interface TabsProps {
    defaultValue?: string;
    value?: string;
    onValueChange?: (value: string) => void;
    children: React.ReactNode;
    className?: string;
}


interface TabsListProps {
    children: React.ReactNode;
    className?: string;
    variant?: 'default' | 'segmented';
}


interface TabsTriggerProps {
    value: string;
    children: React.ReactNode;
    disabled?: boolean;
    className?: string;
}


interface TabsContentProps {
    value: string;
    children: React.ReactNode;
    className?: string;
}
