// Minimal W3C WebDriver HTTP client (tauri-driver → WebKitWebDriver).
//
// Used by the Linux WebKit Tauri driver so the real Release binary can be
// driven end-to-end (ADR-0041 `/security` operational acceptance). Speaks the
// raw W3C WebDriver protocol over HTTP with global fetch — no Playwright
// dependency, because Playwright has no generic WebDriver endpoint.

export interface WebDriverElement {
  readonly id: string;
}

export class WebDriverError extends Error {
  readonly status: number;
  readonly code?: string;

  constructor(status: number, message: string, code?: string) {
    super(message);
    this.name = 'WebDriverError';
    this.status = status;
    this.code = code;
  }
}

const ELEMENT_KEY = 'element-6066-11e4-a52e-4f735466cecf';

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export class WebDriverClient {
  private readonly baseUrl: string;
  private _sessionId: string | null = null;

  constructor(port: number) {
    this.baseUrl = `http://127.0.0.1:${port}`;
  }

  get sessionId(): string | null {
    return this._sessionId;
  }

  /**
   * Open a WebDriver session. On Linux tauri-driver converts the
   * `tauri:options.application` path into `webkitgtk:browserOptions.binary`
   * and WebKitWebDriver launches the Release binary.
   */
  async createSession(application: string): Promise<string> {
    const body = {
      capabilities: {
        alwaysMatch: {
          browserName: 'wry',
          'tauri:options': {
            application,
          },
        },
      },
    };
    const res = await this.request('POST', '/session', body, false);
    const sessionId = (res as { value?: { sessionId?: string } }).value?.sessionId;
    if (!sessionId) {
      throw new WebDriverError(res ? 0 : 500, 'Session response missing sessionId', 'session not created');
    }
    this._sessionId = sessionId;
    return this._sessionId;
  }

  async deleteSession(): Promise<void> {
    if (!this._sessionId) return;
    try {
      await this.request('DELETE', `/session/${this._sessionId}`, null, false);
    } finally {
      this._sessionId = null;
    }
  }

  // ---- navigation & document ----

  async navigate(url: string): Promise<void> {
    await this.request('POST', this.session('/url'), { url });
  }

  async currentUrl(): Promise<string> {
    const res = await this.request('GET', this.session('/url'));
    return res as string;
  }

  async execute<T>(script: string, ...args: unknown[]): Promise<T> {
    // W3C "Execute Script" treats the text as the *body* of a function, so an
    // expression must be wrapped in `return (...)` or the result is always null.
    const res = await this.request('POST', this.session('/execute/sync'), {
      script: `return (${script});`,
      args: args.map((a) => (a === undefined ? null : a)),
    });
    return res as T;
  }

  // ---- elements ----

  /**
   * Find an element. Returns null when no such element exists.
   */
  async find(selector: string): Promise<WebDriverElement | null> {
    try {
      const res = await this.request('POST', this.session('/element'), {
        using: 'css selector',
        value: selector,
      });
      const value = res as Record<string, unknown>;
      const id = value[ELEMENT_KEY] ?? value['ELEMENT'];
      return id ? { id: String(id) } : null;
    } catch (err) {
      if (err instanceof WebDriverError && err.code === 'no such element') {
        return null;
      }
      throw err;
    }
  }

  async findChild(parent: WebDriverElement, selector: string): Promise<WebDriverElement | null> {
    try {
      const res = await this.request(
        'POST',
        this.session(`/element/${parent.id}/element`),
        { using: 'css selector', value: selector },
      );
      const value = res as Record<string, unknown>;
      const id = value[ELEMENT_KEY] ?? value['ELEMENT'];
      return id ? { id: String(id) } : null;
    } catch (err) {
      if (err instanceof WebDriverError && err.code === 'no such element') {
        return null;
      }
      throw err;
    }
  }

  async waitFor(selector: string, timeoutMs = 15000): Promise<WebDriverElement> {
    const start = Date.now();
    for (;;) {
      const el = await this.find(selector);
      if (el) return el;
      if (Date.now() - start > timeoutMs) {
        throw new Error(`Timeout waiting for element: ${selector}`);
      }
      await sleep(200);
    }
  }

  async waitForGone(selector: string, timeoutMs = 15000): Promise<void> {
    const start = Date.now();
    for (;;) {
      const el = await this.find(selector);
      if (!el) return;
      if (Date.now() - start > timeoutMs) {
        throw new Error(`Timeout waiting for element to disappear: ${selector}`);
      }
      await sleep(200);
    }
  }

  async click(selector: string): Promise<void> {
    const el = await this.waitFor(selector);
    await this.request('POST', this.session(`/element/${el.id}/click`), null);
  }

  async clickElement(el: WebDriverElement): Promise<void> {
    await this.request('POST', this.session(`/element/${el.id}/click`), null);
  }

  async clear(selector: string): Promise<void> {
    const el = await this.waitFor(selector);
    await this.request('POST', this.session(`/element/${el.id}/clear`), null);
  }

  async fill(selector: string, text: string): Promise<void> {
    const el = await this.waitFor(selector);
    await this.request('POST', this.session(`/element/${el.id}/clear`), null);
    await this.request('POST', this.session(`/element/${el.id}/value`), { text });
  }

  async text(selector: string): Promise<string> {
    const el = await this.waitFor(selector);
    return this.elementText(el);
  }

  async elementText(el: WebDriverElement): Promise<string> {
    const res = await this.request('GET', this.session(`/element/${el.id}/text`));
    return String(res ?? '');
  }

  async isDisplayed(selector: string): Promise<boolean> {
    const el = await this.find(selector);
    if (!el) return false;
    try {
      const res = await this.request('GET', this.session(`/element/${el.id}/displayed`));
      return res === true;
    } catch {
      return false;
    }
  }

  // ---- raw ----

  private session(path: string): string {
    if (!this._sessionId) throw new Error('No active WebDriver session');
    return `/session/${this._sessionId}${path}`;
  }

  private async request(
    method: string,
    path: string,
    body?: unknown,
    parseValue = true,
  ): Promise<unknown> {
    const headers: Record<string, string> = {
      'content-type': 'application/json',
      accept: 'application/json',
    };
    const init: RequestInit = {
      method,
      headers,
      body:
        body === null || body === undefined
          ? method === 'POST' || method === 'DELETE'
            ? '{}'
            : undefined
          : JSON.stringify(body),
    };

    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}${path}`, init);
    } catch (err) {
      throw new Error(`WebDriver request failed (${method} ${path}): ${String(err)}`);
    }

    let json: { value?: unknown; message?: string } | null = null;
    try {
      json = (await res.json()) as { value?: unknown; message?: string };
    } catch {
      // non-JSON response
    }

    if (!res.ok) {
      const value = json?.value as { error?: string; message?: string; stacktrace?: string } | undefined;
      const message =
        value?.message ??
        json?.message ??
        (json?.value === undefined ? res.statusText : JSON.stringify(json?.value));
      throw new WebDriverError(res.status, message, value?.error);
    }

    if (parseValue) {
      return json?.value;
    }
    return json;
  }
}
