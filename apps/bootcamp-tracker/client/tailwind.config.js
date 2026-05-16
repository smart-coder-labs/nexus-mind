/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      colors: {
        bg: {
          primary: '#0D1117',
          secondary: '#161B22',
          tertiary: '#21262D',
        },
        border: {
          DEFAULT: '#30363D',
          subtle: '#21262D',
        },
        text: {
          primary: '#E6EDF3',
          secondary: '#8B949E',
          muted: '#6E7681',
        },
        accent: {
          DEFAULT: '#58A6FF',
          hover: '#79B8FF',
        },
        success: '#3FB950',
        warning: '#D29922',
        danger: '#F85149',
        priority: {
          p0: '#F85149',
          p1: '#D29922',
          p2: '#6E7681',
        },
      },
    },
  },
  plugins: [],
};
