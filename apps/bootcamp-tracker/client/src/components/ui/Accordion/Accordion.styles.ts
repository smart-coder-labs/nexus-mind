/* ========================================
   ACCORDION - STYLES (cva variants)
   ======================================== */

import { cva } from 'class-variance-authority';

export const accordionVariants = cva('w-full', {
  variants: {
    variant: {
      default: '',
      bordered: 'border border-border-primary rounded-lg',
      separated: 'space-y-2',
    },
  },
  defaultVariants: {
    variant: 'default',
  },
});

export const accordionItemVariants = cva('', {
  variants: {
    variant: {
      default: 'border-b border-border-primary',
      bordered: 'border border-border-primary rounded-lg mb-2',
      separated: 'border border-border-primary rounded-lg',
    },
  },
  defaultVariants: {
    variant: 'default',
  },
});

export const accordionTriggerVariants = cva(
  'flex flex-1 items-center justify-between py-4 font-medium transition-all [&[data-state=open]>svg]:rotate-180',
  {
    variants: {
      variant: {
        default: 'hover:text-accent-blue',
        subtle: 'hover:bg-bg-secondary',
        solid: 'bg-bg-secondary rounded-t-lg',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);

export const accordionContentVariants = cva(
  'overflow-hidden text-sm transition-all data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down',
  {
    variants: {
      variant: {
        default: '',
        padded: 'p-4',
      },
    },
    defaultVariants: {
      variant: 'default',
    },
  }
);