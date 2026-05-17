import React from 'react';
import { Text } from '../Text';
import type { LabelProps } from './Label.types';

export const Label: React.FC<LabelProps> = ({
    children,
    className = '',
    htmlFor,
    required,
    ...props
}) => {
    return (
        <Text
            as="label"
            variant="small"
            weight="medium"
            color="primary"
            className={`block select-none ${className}`}
            {...props}
            // @ts-ignore - htmlFor is valid for label element
            htmlFor={htmlFor}
        >
            {children}
            {required && <span className="text-status-error ml-1">*</span>}
        </Text>
    );
};

Label.displayName = 'Label';
