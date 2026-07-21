export interface ComboboxOption {
    id: string;
    label: string;
    /** Optional leading color dot (any valid CSS color). */
    dotColor?: string;
}

export interface ComboboxProps {
    value: string;
    onChange: (value: string) => void;
    options: ComboboxOption[];
    onSelect: (option: ComboboxOption) => void;
    placeholder?: string;
    noResultsLabel?: string;
    className?: string;
}
