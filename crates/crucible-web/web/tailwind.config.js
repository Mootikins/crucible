/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        /**
         * Primary action colors - use for buttons, links, and interactive elements
         * that represent the main call-to-action or primary user interactions
         */
        primary: {
          DEFAULT: '#3b82f6',  // Standard primary blue
          hover: '#2563eb',    // Darker blue for hover states
          active: '#1e40af',   // Darkest blue for active/pressed states
        },
        /**
         * Surface colors - use for backgrounds and container elements
         * base: main background, elevated: secondary surfaces, overlay: modals/popovers
         */
        surface: {
          base: '#171717',     // Main background (neutral-900)
          elevated: '#1f1f1f', // Secondary background (neutral-800)
          overlay: '#262626',  // Tertiary background for overlays (neutral-700)
        },
        /**
         * Muted colors - use for secondary text, disabled states, and de-emphasized content
         * Provides visual hierarchy by reducing prominence of less important information
         */
        muted: {
          DEFAULT: '#a3a3a3', // Secondary text (neutral-300)
          dark: '#737373',    // Disabled/tertiary text (neutral-500)
        },
        /**
         * Error colors - use exclusively for error states, validation failures, and destructive actions
         * Should be used sparingly to maintain visual impact and user attention
         */
        error: {
          DEFAULT: '#ef4444', // Standard error red
          dark: '#991b1b',    // Dark red for error backgrounds
        },
      },
    },
  },
  plugins: [
    require('@tailwindcss/typography'),
  ],
};
