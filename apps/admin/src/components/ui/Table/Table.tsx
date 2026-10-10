import * as React from "react";
import { Checkbox as PrimitiveCheckbox } from '../shadcn/checkbox';
import { Table as PrimitiveTable, TableHeader, TableBody, TableHead, TableCell, TableRow } from '../shadcn/table';
import { cn } from '../../../lib/utils';
import { motion, useReducedMotion } from "framer-motion";
import {
    ArrowUpDown,
    ChevronLeft,
    ChevronRight,
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

    const totalPages = Math.max(1, Math.ceil(data.length / pageSize));

    const paginatedData = data.slice(
        (page - 1) * pageSize,
        page * pageSize
    );

    const rowPadding =
        density === "compact" ? "py-1.5" : "py-2";

    return (
        <div className="overflow-hidden border border-border-primary bg-surface-primary rounded-xl">
            {/* TABLE */}
            <div className="overflow-x-auto">
            <PrimitiveTable data-density={density} className="admin-data-table w-full border-collapse text-left">
                <TableHeader>
                    <TableRow>
                        {selectable && (
                            <TableHead className="w-10 px-2">
                                <Checkbox
                                    label="Select all rows"
                                    disabled={data.length === 0}
                                    checked={data.length > 0 && selectedRows.size === data.length}
                                    onCheckedChange={toggleAll}
                                />
                            </TableHead>
                        )}

                        {columns.map((col) => (
                            <TableHead
                                key={String(col.key)}
                                style={col.width ? { width: col.width } : undefined}
                                aria-sort={col.sortable ? (sortKey === col.key ? (sortDirection === "asc" ? "ascending" : "descending") : "none") : undefined}
                                className={cn(
                                    "h-10 px-2 text-sm font-medium text-foreground select-none whitespace-nowrap"
                                )}
                            >
                                {col.sortable ? <button
                                    type="button"
                                    className={cn(
                                        "rounded-[4px]",
                                        FOCUS_RING,
                                        col.sortable
                                            ? "flex items-center gap-1 hover:text-text-primary transition-colors"
                                            : "cursor-default",
                                        col.sortable && sortKey === col.key && "text-accent-blue"
                                    )}
                                    onClick={() => handleSort(col)}
                                >
                                    {col.header}

                                    {col.sortable && (
                                        <ArrowUpDown
                                            className={cn(
                                                "h-3 w-3 transition-opacity",
                                                sortKey === col.key
                                                    ? "opacity-100 text-accent-blue"
                                                    : "opacity-40"
                                            )}
                                        />
                                    )}
                                </button> : col.header}
                            </TableHead>
                        ))}
                    </TableRow>
                </TableHeader>

                <TableBody>
                    {paginatedData.length === 0 && (
                        <TableRow>
                            <TableCell
                                colSpan={columns.length + (selectable ? 1 : 0)}
                                className="py-10 text-center text-text-tertiary"
                            >
                                No results found.
                            </TableCell>
                        </TableRow>
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
                                        ? "bg-foreground/[0.02]"
                                        : "",
                                    hoverable &&
                                    "hover:bg-action-primary/[0.05]",
                                    onRowClick && "cursor-pointer"
                                )}
                                onClick={() => onRowClick?.(row)}
                            >
                                {selectable && (
                                    <TableCell className="px-4">
                                        <Checkbox
                                            label={`Select row ${globalIndex + 1}`}
                                            checked={selectedRows.has(globalIndex)}
                                            onCheckedChange={() => toggleRow(globalIndex)}
                                        />
                                    </TableCell>
                                )}

                                {columns.map((col) => (
                                    <TableCell
                                        key={String(col.key)}
                                        className={cn(
                                            "px-2 text-sm text-text-secondary",
                                            rowPadding
                                        )}
                                    >
                                        {col.render
                                            ? col.render(row[col.key], row)
                                            : (row[col.key] as any)}
                                    </TableCell>
                                ))}
                            </motion.tr>
                        );
                    })}
                </TableBody>
            </PrimitiveTable>
            </div>

            {/* PAGINATION */}
            <div className="flex items-center justify-between px-4 py-3 bg-foreground/[0.03] border-t border-border-primary">
                <p className="text-xs text-text-tertiary">
                    Page {page} of {totalPages}
                </p>

                <div className="flex items-center gap-2">
                    <PaginationButton
                        label="Previous page"
                        disabled={page <= 1 || !onPageChange}
                        onClick={() => onPageChange?.(page - 1)}
                    >
                        <ChevronLeft className="w-4 h-4" />
                    </PaginationButton>
                    <PaginationButton
                        label="Next page"
                        disabled={page >= totalPages || !onPageChange}
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
    label,
    disabled,
    children,
    onClick,
}: {
    label: string;
    disabled?: boolean;
    children: React.ReactNode;
    onClick?: () => void;
}) {
    return (
        <button
            type="button"
            aria-label={label}
            disabled={disabled}
            onClick={onClick}
            className={cn(
                "p-2 rounded-md border border-border-primary text-text-tertiary transition-all",
                "hover:border-border-primary hover:text-text-primary",
                FOCUS_RING,
                "disabled:opacity-40 disabled:cursor-not-allowed"
            )}
        >
            {children}
        </button>
    );
}

function Checkbox({ label,checked,onCheckedChange,disabled }:{label:string;checked:boolean;onCheckedChange?:()=>void;disabled?:boolean}) {
 return <PrimitiveCheckbox aria-label={label} checked={checked} disabled={disabled} onCheckedChange={onCheckedChange} onClick={event=>event.stopPropagation()} />;
}
