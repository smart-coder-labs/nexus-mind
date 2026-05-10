/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,ts,tsx}'],
  theme: {
    extend: {
      colors: {
        surface: '#0a0a0f',
        'surface-alt': '#0d0d15',
        accent: '#3b82f6',
        'accent-light': '#60a5fa',
        'green-accent': '#10b981',
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
      },
      keyframes: {
        'pulse-badge': {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.5' },
        },
        'confetti-fall': {
          '0%': { opacity: '1', transform: 'translateY(0) rotate(0deg)' },
          '100%': { opacity: '0', transform: 'translateY(300px) rotate(720deg)' },
        },
      },
      animation: {
        'pulse-badge': 'pulse-badge 2s ease-in-out infinite',
        'confetti-fall': 'confetti-fall 1.5s ease-out forwards',
      },
    },
  },
  plugins: [],
};
