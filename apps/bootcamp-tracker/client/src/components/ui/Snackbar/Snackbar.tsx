import React, { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../../lib/utils';
import { motion, AnimatePresence } from 'framer-motion';
import { X } from 'lucide-react';

/* ========================================
   TYPES
   ======================================== */

export type SnackbarVariant = 'default' | 'success' | 'error' | 'warning' | 'info';

export interface SnackbarAction {
    label: string;
    onClick: () => void;
}

export interface SnackbarProps {
    message: string;
    variant?: SnackbarVariant;
    action?: SnackbarAction;
    duration?: number; // in milliseconds, 0 means no auto-dismiss
    onClose?: () => void;
    show?: boolean;
}

/* ========================================
   SNACKBAR COMPONENT
   ======================================== */

export const Snackbar: React.FC<SnackbarProps> = ({
    message,
    variant = 'default',
    action,
    duration = 5000,
    onClose,
    show = true,
}) => {
    const [mounted, setMounted] = useState(false);

    useEffect(() => {
        setMounted(true);
    }, []);

    useEffect(() => {
        if (show && duration > 0) {
            const timer = setTimeout(() => {
                onClose?.();
            }, duration);

            return () => clearTimeout(timer);
        }
    }, [show, duration, onClose]);

    const variants = {
        default: "bg-surface-elevated text-text-primary border border-border-primary",
        success: "bg-status-success/10 text-status-success border border-status-success/20",
        error: "bg-status-error/10 text-status-error border border-status-error/20",
        warning: "bg-status-warning/10 text-status-warning border border-status-warning/20",
        info: "bg-status-info/10 text-status-info border border-status-info/20",
    };

    const content = (
        <AnimatePresence>
            {show && (
                <motion.div
                    initial={{ y: 100, opacity: 0 }}
                    animate={{ y: 0, opacity: 1 }}
                    exit={{ y: 100, opacity: 0 }}
                    transition={{ type: "spring", stiffness: 400, damping: 30 }}
                    className="fixed bottom-6 left-1/2 -translate-x-1/2 z-30 max-w-md w-full mx-4"
                >
                    <div
                        className={cn(
                            "flex items-center gap-3 px-4 py-3 rounded-xl shadow-lg",
                            variants[variant]
                        )}
                    >
                        <p className="flex-1 text-sm font-medium">{message}</p>

                        <div className="flex items-center gap-2">
                            {action && (
                                <button
                                    onClick={() => {
                                        action.onClick();
                                        onClose?.();
                                    }}
                                    className="text-sm font-semibold hover:opacity-80 transition-opacity focus:outline-none"
                                >
                                    {action.label}
                                </button>
                            )}

                            {onClose && (
                                <button
                                    onClick={onClose}
                                    className="p-1 rounded-lg hover:bg-black/10 transition-apple focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue focus-visible:ring-offset-2"
                                    aria-label="Close"
                                >
                                    <X className="w-4 h-4" />
                                </button>
                            )}
                        </div>
                    </div>
                </motion.div>
            )}
        </AnimatePresence>
    );

    if (!mounted) return null;

    return createPortal(content, document.body);
};

Snackbar.displayName = 'Snackbar';
