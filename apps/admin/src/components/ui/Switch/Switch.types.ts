export interface SwitchProps {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
    label?: string;
    description?: string;
    size?: 'sm' | 'md' | 'lg';
    className?: string;
    'aria-label'?: string;
}
