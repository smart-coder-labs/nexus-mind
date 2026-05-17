import React from 'react';
import { Spinner } from './Spinner/index';
export { Spinner as LoadingSpinner };
export function LoadingPage() {
  return (
    <div className="flex items-center justify-center h-64">
      <Spinner size="lg" />
    </div>
  );
}
