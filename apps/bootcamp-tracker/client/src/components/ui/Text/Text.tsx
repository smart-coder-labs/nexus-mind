import React from 'react';

export type TextVariant = 'body' | 'small' | 'tiny' | 'lead';
export type TextWeight = 'normal' | 'medium' | 'semibold' | 'bold';
export type TextAlign = 'left' | 'center' | 'right' | 'justify';
export type TextColor = 'primary' | 'secondary' | 'tertiary' | 'quaternary' | 'inverse' | 'accent' | 'success' | 'warning' | 'error';

export interface TextProps {
    variant?: TextVariant;
    weight?: TextWeight;
    align?: TextAlign;
    color?: TextColor;
    italic?: boolean;
    underline?: boolean;
    truncate?: boolean;
    lineClamp?: number;
    as?: 'p' | 'span' | 'div' | 'label';
    className?: string;
    children?: React.ReactNode;
}

const variantStyles: Record<TextVariant, string> = {
    lead: 'text-xl leading-relaxed',
    body: 'text-base leading-normal',
    small: 'text-sm leading-normal',
    tiny: 'text-xs leading-tight',
};

const weightStyles: Record<TextWeight, string> = {
    normal: 'font-normal',
    medium: 'font-medium',
    semibold: 'font-semibold',
    bold: 'font-bold',
};

const alignStyles: Record<TextAlign, string> = {
    left: 'text-left',
    center: 'text-center',
    right: 'text-right',
    justify: 'text-justify',
};

const colorStyles: Record<TextColor, string> = {
    primary: 'text-text-primary',
    secondary: 'text-text-secondary',
    tertiary: 'text-text-tertiary',
    quaternary: 'text-text-quaternary',
    inverse: 'text-text-inverse',
    accent: 'text-accent-blue',
    success: 'text-status-success',
    warning: 'text-status-warning',
    error: 'text-status-error',
};

export const Text: React.FC<TextProps> = ({
    variant = 'body',
    weight = 'normal',
    align = 'left',
    color = 'primary',
    italic = false,
    underline = false,
    truncate = false,
    lineClamp,
    as: Component = 'p',
    className = '',
    children,
    ...props
}) => {
    const combinedClassName = `
    ${variantStyles[variant]}
    ${weightStyles[weight]}
    ${alignStyles[align]}
    ${colorStyles[color]}
    ${italic ? 'italic' : ''}
    ${underline ? 'underline' : ''}
    ${truncate ? 'truncate' : ''}
    ${lineClamp ? `line-clamp-${lineClamp}` : ''}
    ${className}
  `.trim().replace(/\s+/g, ' ');

    return (
        <Component className={combinedClassName} {...props}>
            {children}
        </Component>
    );
};

Text.displayName = 'Text';

