/**
 * Disposable stack — unified cleanup for component/page lifecycles.
 */

export type Disposable = () => void;

export class DisposableStack {
  private disposables: Disposable[] = [];
  private disposed = false;

  add(disposable: Disposable): void {
    if (this.disposed) {
      disposable();
      return;
    }
    this.disposables.push(disposable);
  }

  addAbortController(controller: AbortController): AbortSignal {
    this.add(() => controller.abort());
    return controller.signal;
  }

  isDisposed(): boolean {
    return this.disposed;
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    while (this.disposables.length > 0) {
      const fn = this.disposables.pop();
      try {
        fn?.();
      } catch {
        // Cleanup must never throw
      }
    }
  }
}
