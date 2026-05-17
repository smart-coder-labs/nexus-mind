/* ========================================
   LABEL - TYPES
   ======================================== */

import type { TextProps } from '../Text';

export interface LabelProps extends Omit<TextProps, 'as'> {
    htmlFor?: string;
    required?: boolean;
}