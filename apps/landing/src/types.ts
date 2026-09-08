/// <reference types="astro/client" />

interface Window {
  showToast: (msg: string, isError?: boolean) => void;
  confetti: () => void;
}
