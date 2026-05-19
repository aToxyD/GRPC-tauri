import { writable, get } from 'svelte/store';

export type Theme = 'light' | 'dark';

function createThemeStore() {
	const { subscribe, set } = writable<Theme>('dark');

	return {
		subscribe,
		set: (value: Theme) => {
			if (typeof window !== 'undefined') {
				localStorage.setItem('theme', value);
				if (value === 'dark') {
					document.documentElement.classList.add('dark');
				} else {
					document.documentElement.classList.remove('dark');
				}
			}
			set(value);
		},
		toggle: () => {
			const current = get(theme);
			const nextTheme = current === 'light' ? 'dark' : 'light';
			if (typeof window !== 'undefined') {
				localStorage.setItem('theme', nextTheme);
				if (nextTheme === 'dark') {
					document.documentElement.classList.add('dark');
				} else {
					document.documentElement.classList.remove('dark');
				}
			}
			set(nextTheme);
		}
	};
}

export const theme = createThemeStore();

export function initializeTheme() {
	if (typeof window !== 'undefined') {
		const storedTheme = localStorage.getItem('theme') as Theme | null;
		const initialTheme: Theme = storedTheme === 'light' ? 'light' : 'dark';

		theme.set(initialTheme);
	}
}

export function toggleTheme() {
	theme.toggle();
}
