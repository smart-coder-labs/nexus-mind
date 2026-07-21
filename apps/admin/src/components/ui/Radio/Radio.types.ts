export interface RadioOption {
    value: string;
    label: string;
    disabled?: boolean;
}

export interface RadioGroupProps {
    name: string;
    value: string;
    onChange: (value: string) => void;
    options: RadioOption[];
    disabled?: boolean;
    className?: string;
}

export interface RadioProps {
    name: string;
    value: string;
    checked: boolean;
    onChange: (value: string) => void;
    label?: React.ReactNode;
    disabled?: boolean;
    className?: string;
}
