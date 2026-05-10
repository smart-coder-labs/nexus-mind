/// <reference types="astro/client" />

interface WaitlistEntry {
  name: string;
  email: string;
  company: string;
  size: string;
  interests: string[];
  message: string;
  timestamp: string;
}

interface Window {
  showToast: (msg: string, isError?: boolean) => void;
  confetti: () => void;
}
