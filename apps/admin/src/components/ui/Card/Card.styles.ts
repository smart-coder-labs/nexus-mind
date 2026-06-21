/* ========================================
   CARD - STYLES
   ======================================== */

import type { CardVariant } from './Card.types';

export const cardBaseStyles = `
  rounded-[18px] transition-apple
`;

export const cardVariantStyles: Record<CardVariant, string> = {
    elevated: `
    bg-[#1d1d1f]
    shadow-md
    hover:shadow-lg
  `,
    glass: `
    glass
    border border-border-secondary
    shadow-sm
  `,
    outlined: `
    bg-[#1d1d1f]
    border border-border-primary
    hover:border-border-primary
  `,
    flat: `
    bg-[#272729]
  `,
};

export const cardPaddingStyles = {
    none: '',
    sm: 'p-4',
    md: 'p-6',
    lg: 'p-8',
};