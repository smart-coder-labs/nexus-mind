import * as React from "react";
import { cn } from '../../../lib/utils';;
import { motion, useReducedMotion } from "framer-motion";
import {
    ArrowUpDown,
    ChevronLeft,
    ChevronRight,
    Check,
} from "lucide-react";

// Visible keyboard-focus indicator (DESIGN_DIRECTION §6).
const FOCUS_RING =
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring";

/* -------------------------------------------------------------------------- */
/*                                   TYPES                                    */
/* -------------------------------------------------------------------------- */

export type Column<T> = {
    key: keyof T;
    header: string;
    width?: string;
    sortable?: boolean;
    render?: (value: any, row: T) => React.ReactNode;
};

export interface TableProps<T> {
    columns: Column<T>[];
    data: T[];
    selectable?: boolean;
    striped?: boolean;
    hoverable?: boolean;
    density?: "comfortable" | "compact";
    page?: number;
    pageSize?: number;
    onPageChange?: (page: number) => void;
    onSortChange?: (key: keyof T, direction: "asc" | "desc") => void;
    onRowClick?: (row: T) => void;
}

/* -------------------------------------------------------------------------- */
/*                               ROOT COMPONENT                               */
/* -------------------------------------------------------------------------- */

export function Table<T>({
    columns,
    data,
    selectable = false,
    striped = false,
    hoverable = true,
    density = "comfortable",
    page = 1,
    pageSize = 10,
    onPageChange,
    onSortChange,
    onRowClick,
}: TableProps<T>) {
    const [sortKey, setSortKey] = React.useState<keyof T | null>(null);
    const [sortDirection, setSortDirection] = React.useState<"asc" | "desc">(
        "asc"
    );
    const [selectedRows, setSelectedRows] = React.useState<Set<number>>(new Set());
    const prefersReducedMotion = useReducedMotion();

    const handleSort = (col: Column<T>) => {
        if (!col.sortable) return;
        const newDirection =
            sortKey === col.key && sortDirection === "asc" ? "desc" : "asc";

        setSortKey(col.key);
        setSortDirection(newDirection);
        onSortChange?.(col.key, newDirection);
    };

    const toggleRow = (index: number) => {
        const copy = new Set(selectedRows);
        copy.has(index) ? copy.delete(index) : copy.add(index);
        setSelectedRows(copy);
    };

    const toggleAll = () => {
        if (selectedRows.size === data.length) {
            setSelectedRows(new Set());
        } else {
            setSelectedRows(new Set(data.map((_, i) => i)));
        }
    };

    const totalPages = Math.ceil(data.length / pageSize);

    const paginatedData = data.slice(
        (page - 1) * pageSize,
        page * pageSize
    );

    const rowPadding =
        density === "compact" ? "py-2" : "py-3";

    return (
        <div className="overflow-hidden border border-border-primary rounded-[18px] bg-background-secondary">
            {/* TABLE */}
            <table className="w-full border-collapse text-left">
                <thead className="bg-background-tertiary/50 border-b border-border-primary">
                    <tr>
                        {selectable && (
                            <th className="w-10 px-4">
                                <Checkbox
                                    checked={selectedRows.size === data.length}
                                    onCheckedChange={toggleAll}
                                />
                            </th>
                        )}

                        {columns.map((col) => (
                            <th
                                key={String(col.key)}
                                className={cn(
                                    "px-4 py-3 text-xs font-medium text-text-tertiary uppercase tracking-wide select-none whitespace-nowrap",
                                    col.width && `w-[${col.width}]`
                                )}
                            >
                                <button
                                    className={cn(
                                        "rounded-[4px]",
                                        FOCUS_RING,
                                        col.sortable
                                            ? "flex items-center gap-1 hover:text-text-primary transition-colors"
                                            : "cursor-default"
                                    )}
                                    onClick={() => handleSort(col)}
                                >
                                    {col.header}

                                    {col.sortable && (
                                        <ArrowUpDown
                                            className={cn(
                                                "h-3 w-3 transition-opacity",
                                                sortKey === col.key
                                                    ? "opacity-100 text-text-primary"
                                                    : "opacity-40"
                                            )}
                                        />
                                    )}
                                </button>
                            </th>
                        ))}
                    </tr>
                </thead>

                <tbody>
                    {paginatedData.length === 0 && (
                        <tr>
                            <td
                                colSpan={columns.length + (selectable ? 1 : 0)}
                                className="py-10 text-center text-text-tertiary"
                            >
                                No results found.
                            </td>
                        </tr>
                    )}

                    {paginatedData.map((row, index) => {
                        const globalIndex = (page - 1) * pageSize + index;

                        return (
                            <motion.tr
                                key={globalIndex}
                                initial={prefersReducedMotion ? false : { opacity: 0 }}
                                animate={{ opacity: 1 }}
                                transition={{ duration: prefersReducedMotion ? 0 : 0.18 }}
                                className={cn(
                                    "border-b border-border-secondary transition-colors",
                                    striped && index % 2 === 1
                                        ? "bg-background-tertiary/40"
                                        : "",
                                    hoverable &&
                                    "hover:bg-white/[0.03] cursor-pointer"
                                )}
                                onClick={() => onRowClick?.(row)}
                            >
                                {selectable && (
                                    <td className="px-4">
                                        <Checkbox
                                            checked={selectedRows.has(globalIndex)}
                                            onCheckedChange={() => toggleRow(globalIndex)}
                                        />
                                    </td>
                                )}

                                {columns.map((col) => (
                                    <td
                                        key={String(col.key)}
                                        className={cn(
                                            "px-4 text-[13px] text-text-secondary",
                                            rowPadding
                                        )}
                                    >
                                        {col.render
                                            ? col.render(row[col.key], row)
                                            : (row[col.key] as any)}
                                    </td>
                                ))}
                            </motion.tr>
                        );
                    })}
                </tbody>
            </table>

            {/* PAGINATION */}
            <div className="flex items-center justify-between px-4 py-3 bg-background-tertiary/40 border-t border-border-primary">
                <p className="text-xs text-text-tertiary">
                    Page {page} of {totalPages}
                </p>

                <div className="flex items-center gap-2">
                    <PaginationButton
                        disabled={page === 1}
                        onClick={() => onPageChange?.(page - 1)}
                    >
                        <ChevronLeft className="w-4 h-4" />
                    </PaginationButton>
                    <PaginationButton
                        disabled={page === totalPages}
                        onClick={() => onPageChange?.(page + 1)}
                    >
                        <ChevronRight className="w-4 h-4" />
                    </PaginationButton>
                </div>
            </div>
        </div>
    );
}

/* -------------------------------------------------------------------------- */
/*                               SUB COMPONENTS                               */
/* -------------------------------------------------------------------------- */

function PaginationButton({
    disabled,
    children,
    onClick,
}: {
    disabled?: boolean;
    children: React.ReactNode;
    onClick?: () => void;
}) {
    return (
        <button
            disabled={disabled}
            onClick={onClick}
            className={cn(
                "p-2 rounded-[8px] border border-border-primary text-text-secondary transition-all",
                "hover:bg-background-tertiary hover:text-text-primary",
                FOCUS_RING,
                "disabled:opacity-40 disabled:cursor-not-allowed"
            )}
        >
            {children}
        </button>
    );
}

function Checkbox({
    checked,
    onCheckedChange,
    disabled,
}: {
    checked: boolean;
    onCheckedChange?: () => void;
    disabled?: boolean;
}) {
    return (
        <button
            type="button"
            role="checkbox"
            aria-checked={checked}
            aria-disabled={disabled || undefined}
            data-state={checked ? "checked" : "unchecked"}
            onClick={() => {
                if (disabled) return;
                onCheckedChange?.();
            }}
            onKeyDown={(event) => {
                if (disabled) return;
                if (event.key === " " || event.key === "Enter") {
                    event.preventDefault();
                    onCheckedChange?.();
                }
            }}
            className={cn(
                "h-4 w-4 rounded-[5px] border border-border-primary bg-white/[0.04] flex items-center justify-center",
                "data-[state=checked]:bg-accent-blue data-[state=checked]:border-accent-blue",
                "transition-colors",
                FOCUS_RING,
                disabled && "opacity-50 cursor-not-allowed"
            )}
        >
            {checked && <Check className="h-3 w-3 text-white" />}
        </button>
    );
}
