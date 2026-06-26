/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.rs", "./index.html"],
  theme: {
    extend: {
      fontFamily: {
        display: ['"Bebas Neue"', 'sans-serif'],
        sans: ['Barlow', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'sans-serif'],
      },
      colors: {
        sc: {
          base:              'var(--sc-base)',
          panel:             'var(--sc-panel)',
          card:              'var(--sc-card)',
          border:            'var(--sc-border)',
          'border-input':    'var(--sc-border-input)',
          accent:            'var(--sc-accent)',
          'accent-hover':    'var(--sc-accent-hover)',
          'accent-dim':      'var(--sc-accent-dim)',
          'accent-bg':       'var(--sc-accent-bg)',
          'accent-bg-hover': 'var(--sc-accent-bg-hover)',
          'accent-deep':     'var(--sc-accent-deep)',
          'accent-border':   'var(--sc-accent-border)',
        },
      },
    },
  },
  plugins: [],
}
