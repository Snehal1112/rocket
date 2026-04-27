import tailwindcssAnimate from 'tailwindcss-animate'
import typography from '@tailwindcss/typography'

/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
  	extend: {
  		colors: {
  			border: 'hsl(var(--border))',
  			input: 'hsl(var(--input))',
  			ring: 'hsl(var(--ring))',
  			background: 'hsl(var(--background))',
  			foreground: 'hsl(var(--foreground))',
  			primary: {
  				DEFAULT: 'hsl(var(--primary))',
  				foreground: 'hsl(var(--primary-foreground))'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary))',
  				foreground: 'hsl(var(--secondary-foreground))'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive))',
  				foreground: 'hsl(var(--destructive-foreground))'
  			},
  			warning: {
  				DEFAULT: 'hsl(var(--warning))',
  				foreground: 'hsl(var(--warning-foreground))'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted))',
  				foreground: 'hsl(var(--muted-foreground))'
  			},
  			accent: {
  				DEFAULT: 'hsl(var(--accent))',
  				foreground: 'hsl(var(--accent-foreground))'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover))',
  				foreground: 'hsl(var(--popover-foreground))'
  			},
  			card: {
  				DEFAULT: 'hsl(var(--card))',
  				foreground: 'hsl(var(--card-foreground))'
  			},
  			chart: {
  				'1': 'hsl(var(--chart-1))',
  				'2': 'hsl(var(--chart-2))',
  				'3': 'hsl(var(--chart-3))',
  				'4': 'hsl(var(--chart-4))',
  				'5': 'hsl(var(--chart-5))'
  			},
  			statusbar: {
  				bg:          'hsl(var(--statusbar-bg))',
  				border:      'hsl(var(--statusbar-border))',
  				'item-hover':  'hsl(var(--statusbar-item-hover))',
  				'item-active': 'hsl(var(--statusbar-item-active))',
  			},
  			breadcrumb: {
  				bg:         'hsl(var(--breadcrumb-bg))',
  				fg:         'hsl(var(--breadcrumb-fg))',
  				'focus-fg': 'hsl(var(--breadcrumb-focus-fg))',
  			},
  			'button-hover-bg':        'hsl(var(--button-hover-bg))',
  			'button-border':          'hsl(var(--button-border))',
  			'button-secondary-bg':    'hsl(var(--button-secondary-bg))',
  			'button-secondary-hover': 'var(--button-secondary-hover-bg)',
  			checkbox: {
  				bg:     'hsl(var(--checkbox-bg))',
  				border: 'hsl(var(--checkbox-border))',
  				fg:     'hsl(var(--checkbox-fg))',
  			},
  			dropdown: {
  				bg:     'hsl(var(--dropdown-bg))',
  				border: 'hsl(var(--dropdown-border))',
  				fg:     'hsl(var(--dropdown-fg))',
  			},
  			badge: {
  				bg: 'hsl(var(--badge-bg))',
  				fg: 'hsl(var(--badge-fg))',
  			},
  			list: {
  				'active-bg':    'hsl(var(--list-active-selection-bg))',
  				'active-fg':    'hsl(var(--list-active-selection-fg))',
  				'inactive-bg':  'hsl(var(--list-inactive-selection-bg))',
  				'hover-bg':     'hsl(var(--list-hover-bg))',
  				'focus-bg':     'hsl(var(--list-focus-bg))',
  				'highlight-fg': 'hsl(var(--list-highlight-fg))',
  				'error-fg':     'hsl(var(--list-error-fg))',
  				'warning-fg':   'hsl(var(--list-warning-fg))',
  			},
  			panel: {
  				bg:                    'hsl(var(--panel-bg))',
  				border:                'hsl(var(--panel-border))',
  				'title-active-border': 'hsl(var(--panel-title-active-border))',
  				'title-active-fg':     'hsl(var(--panel-title-active-fg))',
  				'title-inactive-fg':   'hsl(var(--panel-title-inactive-fg))',
  			},
  			sidebar: {
  				bg:     'hsl(var(--sidebar-bg))',
  				border: 'hsl(var(--sidebar-border))',
  			},
  			titlebar: {
  				bg:     'hsl(var(--titlebar-bg))',
  				fg:     'hsl(var(--titlebar-fg))',
  				border: 'hsl(var(--titlebar-border))',
  			},
  			'disabled-foreground': 'hsl(var(--disabled-foreground))',
  			'error-foreground':    'hsl(var(--error-foreground))',
  			'text-link':           'hsl(var(--text-link))',
  			'text-block-quote-bg': 'hsl(var(--text-block-quote-bg))',
  			'text-code-bg':        'hsl(var(--text-code-bg))',
  			'text-preformat-fg':   'hsl(var(--text-preformat-fg))',
  			git: {
  				added:      'hsl(var(--git-added))',
  				modified:   'hsl(var(--git-modified))',
  				deleted:    'hsl(var(--git-deleted))',
  				untracked:  'hsl(var(--git-untracked))',
  				ignored:    'hsl(var(--git-ignored))',
  				conflicted: 'hsl(var(--git-conflicted))',
  			},
  			tab: {
  				'active-bg':         'hsl(var(--tab-active-bg))',
  				'inactive-bg':       'hsl(var(--tab-inactive-bg))',
  				'active-border-top': 'hsl(var(--tab-active-border-top))',
  			},
  			toolbar: {
  				hover: 'var(--toolbar-hover)',
  			}
  		},
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		}
  	}
  },
  plugins: [tailwindcssAnimate, typography],
}
