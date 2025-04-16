/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./web/**/*.{html,js}",
    "./web/css/**/*.css"
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        primary: '#4361ee',
        'primary-light': '#7895ff',
        'primary-dark': '#2d46cc',
        secondary: '#3f37c9',
        accent: '#4895ef',
        success: '#15c39a',
        warning: '#f8a93a',
        danger: '#f15bb5',
      },
      animation: {
        'spin': 'spin 1s linear infinite',
        'pulse': 'pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        'fade-in': 'fadeIn 0.3s ease-out',
        'slide-in': 'modalSlideIn 0.35s ease-out',
      },
      keyframes: {
        fadeIn: {
          'from': { opacity: 0, transform: 'translateY(12px)' },
          'to': { opacity: 1, transform: 'translateY(0)' },
        },
        modalSlideIn: {
          'from': { transform: 'translateY(-20px)', opacity: 0 },
          'to': { transform: 'translateY(0)', opacity: 1 },
        },
      },
      boxShadow: {
        'inner-top': 'inset 0 1px 2px 0 rgb(0 0 0 / 0.05)',
        'smooth-md': '0 4px 6px rgba(0, 0, 0, 0.04), 0 2px 10px rgba(0, 0, 0, 0.03)',
        'smooth-lg': '0 10px 15px rgba(0, 0, 0, 0.04), 0 4px 6px rgba(0, 0, 0, 0.02)',
        'smooth-xl': '0 20px 25px rgba(0, 0, 0, 0.05), 0 10px 10px rgba(0, 0, 0, 0.02)',
      },
      transitionProperty: {
        'max-height': 'max-height',
      },
      transitionDuration: {
        '250': '250ms',
        '350': '350ms',
      },
      borderRadius: {
        'xl': '1rem',
        '2xl': '1.5rem',
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'sans-serif'],
      },
    },
  },
  plugins: [],
}