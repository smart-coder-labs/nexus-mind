/* ========================================
   CARD - STYLES
   ======================================== */

import type { CardVariant } from './Card.types';

export const cardBaseStyles = `
  rounded-[18px] transition-apple
`;

export const cardVariantStyles: Record<CardVariant, string> = {
    elevated: `
    border border-white/[0.07]
    bg-[#0d0f14]/60
    backdrop-blur-[12px]
    shadow-md
    hover:shadow-lg
  `,
    glass: `
    glass
    border border-white/[0.07]
    shadow-sm
  `,
    outlined: `
    border border-white/[0.07]
    bg-[#0d0f14]/60
    backdrop-blur-[12px]
    hover:border-white/[0.07]
  `,
    flat: `
    border border-white/[0.07]
    bg-[#0d0f14]/60
    backdrop-blur-[12px]
  `,
};

export const cardPaddingStyles = {
    none: '',
    sm: 'p-4',
    md: 'p-6',
    lg: 'p-8',
};