import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { get } from 'svelte/store';
import { theme, toggleTheme, initializeTheme } from '../../lib/theme';

/**
 * اختبارات نظام الثيم (Theme System Tests)
 *
 * تغطي:
 * - الثيم الافتراضي (داكن)
 * - الاستمرارية عبر localStorage
 * - منطق الـ fallback
 * - سلوك التبديل
 * - تطبيق class على document.documentElement
 * - prefers-reduced-motion (لا يؤثر على الثيم لكن مرتبط بالوصول)
 */
describe('نظام الثيم — Theme System', () => {
	beforeEach(() => {
		// محاكاة localStorage
		const store = new Map<string, string>();
		vi.stubGlobal('localStorage', {
			getItem: (key: string) => store.get(key) || null,
			setItem: (key: string, value: string) => store.set(key, value),
			clear: () => store.clear(),
			removeItem: (key: string) => store.delete(key),
		});

		// محاكاة matchMedia (النظام: فاتح افتراضياً)
		vi.stubGlobal('matchMedia', (query: string) => ({
			matches: false,
			media: query,
			onchange: null,
			addListener: vi.fn(),
			removeListener: vi.fn(),
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			dispatchEvent: vi.fn(),
		}));

		// تهيئة DOM نظيف
		document.documentElement.className = '';
		// الثيم الافتراضي في المتجر: داكن
		theme.set('dark');
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	// ─── 1. الثيم الافتراضي ──────────────────────────────────────
	it('يجب أن يكون الثيم الافتراضي داكناً عند غياب تفضيل محفوظ', () => {
		initializeTheme();
		expect(get(theme)).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);
	});

	// ─── 2. تفضيل النظام — داكن ───────────────────────────────────
	it('يجب تفعيل الثيم الداكن عند تفضيل النظام له', () => {
		vi.stubGlobal('matchMedia', (query: string) => ({
			matches: query === '(prefers-color-scheme: dark)',
			media: query,
		}));
		initializeTheme();
		expect(get(theme)).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);
	});

	// ─── 3. تفضيل النظام — فاتح مع غياب تفضيل محفوظ ─────────────
	it('يجب تفعيل الثيم الداكن كـ fallback حتى عند تفضيل النظام الفاتح', () => {
		// لا يوجد تفضيل محفوظ، والنظام فاتح → يجب أن يبقى داكناً (افتراضي المنصة)
		vi.stubGlobal('matchMedia', (query: string) => ({
			matches: false, // النظام: فاتح
			media: query,
		}));
		initializeTheme();
		expect(get(theme)).toBe('dark');
	});

	// ─── 4. الاستمرارية: حفظ واسترجاع ────────────────────────────
	it('يجب حفظ الثيم في localStorage عند التغيير', () => {
		theme.set('dark');
		expect(localStorage.getItem('theme')).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);

		theme.set('light');
		expect(localStorage.getItem('theme')).toBe('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});

	// ─── 5. التفضيل المحفوظ يتجاوز تفضيل النظام ──────────────────
	it('يجب أن يأخذ الثيم المحفوظ الأولوية على تفضيل النظام', () => {
		localStorage.setItem('theme', 'light');
		vi.stubGlobal('matchMedia', () => ({ matches: true })); // النظام: داكن

		initializeTheme();
		expect(get(theme)).toBe('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});

	// ─── 6. الثيم الداكن المحفوظ يُستعاد بشكل صحيح ───────────────
	it('يجب استعادة الثيم الداكن المحفوظ بعد إعادة التشغيل', () => {
		localStorage.setItem('theme', 'dark');
		vi.stubGlobal('matchMedia', () => ({ matches: false })); // النظام: فاتح

		initializeTheme();
		expect(get(theme)).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);
	});

	// ─── 7. التبديل الصحيح ────────────────────────────────────────
	it('يجب التبديل من فاتح إلى داكن بشكل صحيح', () => {
		theme.set('light');

		toggleTheme();
		expect(get(theme)).toBe('dark');
		expect(localStorage.getItem('theme')).toBe('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);

		toggleTheme();
		expect(get(theme)).toBe('light');
		expect(localStorage.getItem('theme')).toBe('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});

	// ─── 8. إزالة class عند التحويل للفاتح ───────────────────────
	it('يجب إزالة class dark من documentElement عند التحويل للثيم الفاتح', () => {
		theme.set('dark');
		expect(document.documentElement.classList.contains('dark')).toBe(true);

		theme.set('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});

	// ─── 9. عدم تكرار class dark ──────────────────────────────────
	it('يجب ألا يُكرر class dark عند تعيين الثيم الداكن مرتين', () => {
		theme.set('dark');
		theme.set('dark');
		const darkClasses = document.documentElement.classList;
		// classList يمنع التكرار تلقائياً — نتحقق أن العدد صحيح
		expect([...darkClasses].filter(c => c === 'dark').length).toBe(1);
	});

	// ─── 10. تسلسل التبديل المتعدد ────────────────────────────────
	it('يجب أن يتعامل مع تسلسل تبديلات متعددة بشكل صحيح', () => {
		theme.set('dark');
		toggleTheme(); // → light
		toggleTheme(); // → dark
		toggleTheme(); // → light

		expect(get(theme)).toBe('light');
		expect(document.documentElement.classList.contains('dark')).toBe(false);
	});
});
