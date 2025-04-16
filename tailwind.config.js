/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./web/**/*.html",
    "./web/js/**/*.js",
  ],
  darkMode: 'class', // Enable dark mode based on class
  theme: {
    extend: {
      colors: {
        // Define colors using CSS variables from base.css
        primary: 'var(--color-primary)',
        secondary: 'var(--color-secondary)',
        success: 'var(--color-success)',
        warning: 'var(--color-warning)',
        danger: 'var(--color-danger)',
        // Add shades for background, text, borders etc.
        gray: {
          50: 'var(--color-gray-50)',
          100: 'var(--color-gray-100)',
          200: 'var(--color-gray-200)',
          300: 'var(--color-gray-300)',
          400: 'var(--color-gray-400)',
          500: 'var(--color-gray-500)',
          600: 'var(--color-gray-600)',
          700: 'var(--color-gray-700)',
          800: 'var(--color-gray-800)',
          900: 'var(--color-gray-900)',
        },
        blue: { // Example primary shade mapping
          500: 'var(--color-primary)',
          // Add other shades if needed
        },
        indigo: { // Example secondary shade mapping
          500: 'var(--color-secondary)',
           // Add other shades if needed
        },
        amber: { // Example warning shade mapping
           50: 'var(--color-amber-50)',
           100: 'var(--color-amber-100)',
           400: 'var(--color-warning)',
           600: 'var(--color-amber-600)',
           800: 'var(--color-amber-800)',
           900: 'var(--color-amber-900)',
        },
         red: { // Example danger shade mapping
           50: 'var(--color-red-50)',
           100: 'var(--color-red-100)',
           200: 'var(--color-red-200)',
           500: 'var(--color-danger)',
           800: 'var(--color-red-800)',
           900: 'var(--color-red-900)',
        },
         purple: { // For tool calls
           100: 'var(--color-purple-100)',
           400: 'var(--color-purple-400)',
           500: 'var(--color-purple-500)',
           700: 'var(--color-purple-700)',
           900: 'var(--color-purple-900)',
         }
      },
      spacing: {
        // Define spacing using CSS variables if needed, or use Tailwind defaults
        // Example: '4': 'var(--space-4)',
      },
      borderRadius: {
        // Define border radius using CSS variables if needed
        // Example: 'lg': 'var(--radius-lg)',
        'xl': '0.75rem', // Keep consistent with existing usage
        '2xl': '1rem',
      },
      boxShadow: {
        // Define shadows using CSS variables if needed
        // Example: 'lg': 'var(--shadow-lg)',
        'md': '0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)',
        'lg': '0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)',
        'xl': '0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1)',
        'inner': 'inset 0 2px 4px 0 rgb(0 0 0 / 0.05)',
      },
      keyframes: {
        // Define animations directly or use tailwindcss-animate plugin
        fadeIn: {
          '0%': { opacity: '0', transform: 'translateY(10px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        modalFadeIn: {
          'from': { opacity: '0' },
          'to': { opacity: '1' },
        },
        modalSlideIn: {
          'from': { transform: 'translateY(-30px)', opacity: '0' },
          'to': { transform: 'translateY(0)', opacity: '1' },
        },
        pulse: {
          '0%, 100%': { boxShadow: '0 0 0 0 rgba(59, 130, 246, 0.4)' }, // Use primary color var if defined
          '70%': { boxShadow: '0 0 0 6px rgba(59, 130, 246, 0)' },
        },
        spin: {
          'to': { transform: 'rotate(360deg)' },
        },
      },
      animation: {
        fadeIn: 'fadeIn 0.3s ease-out forwards',
        modalFadeIn: 'modalFadeIn 0.25s ease-out forwards',
        modalSlideIn: 'modalSlideIn 0.35s ease-out forwards',
        pulse: 'pulse 1.5s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        spin: 'spin 1s linear infinite',
      },
    },
  },
  plugins: [
    require('@tailwindcss/forms'),
    // require('tailwindcss-animate'), // Uncomment if using this plugin
  ],
};
