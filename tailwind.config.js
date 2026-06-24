/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./crates/frontend/src/**/*.rs",
    "./index.html",
  ],
  theme: {
    extend: {
      fontFamily: {
        display: ['"Bebas Neue"', 'sans-serif'],
        sans: ['Barlow', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'sans-serif'],
      },
      colors: {
        // Sorcerer palette tokens — backed by CSS custom properties on :root.
        // Theme switching: update CSS vars, all sc-* classes follow automatically.
        sc: {
          base:            'var(--sc-base)',           // deepest bg (#0d0906)
          panel:           'var(--sc-panel)',          // header/footer/filter bar (#17100a)
          card:            'var(--sc-card)',           // movie cards, inputs (#201610)
          border:          'var(--sc-border)',         // standard dividers (#2b1e14)
          'border-input':  'var(--sc-border-input)',  // select/input borders (#342517)
          accent:          'var(--sc-accent)',         // primary text accent, orange-600
          'accent-hover':  'var(--sc-accent-hover)',  // lighter accent hover, orange-500
          'accent-dim':    'var(--sc-accent-dim)',     // footer link hover, orange-700
          'accent-bg':     'var(--sc-accent-bg)',     // button/badge fill, orange-700
          'accent-bg-hover': 'var(--sc-accent-bg-hover)', // button hover fill, orange-600
          'accent-deep':   'var(--sc-accent-deep)',   // deep badge bg, orange-950
          'accent-border': 'var(--sc-accent-border)', // badge border / ring, orange-800
        },
      },
    },
  },
  plugins: [],
}
