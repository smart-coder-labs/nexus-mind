import React from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { Monitor, Moon, Sun } from 'lucide-react';
import { Switch } from '../Switch';
import { cn } from '../../../lib/utils';

/* ========================================
   TYPES
   ======================================== */

type ThemeName = 'light' | 'dark';
export type ThemeMode = ThemeName | 'system';

export interface ThemeToggleProps {
    /** Modo inicial. `system` respeta la preferencia del SO. */
    defaultMode?: ThemeMode;
    /** Clave usada para persistir la preferencia en `localStorage`. */
    storageKey?: string;
    /** Texto principal del control. */
    label?: string;
    /** Texto secundario opcional. Si no se define, se genera automáticamente. */
    description?: string;
    /** Permite volver al modo automático (prefiere color scheme del sistema). */
    allowSystem?: boolean;
    /** Clase adicional para el contenedor. */
    className?: string;
    /** Callback al cambiar el modo. */
    onModeChange?: (mode: ThemeMode, theme: ThemeName) => void;
}

/* ========================================
   HELPERS
   ======================================== */

const isBrowser = typeof window !== 'undefined';

const resolveTheme = (mode: ThemeMode): ThemeName => {
    if (mode === 'system') {
        if (!isBrowser) return 'light';
        return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    return mode;
};

const isValidMode = (value: string | null): value is ThemeMode =>
    value === 'light' || value === 'dark' || value === 'system';

/* ========================================
   COMPONENT
   ======================================== */

export const ThemeToggle: React.FC<ThemeToggleProps> = ({
    defaultMode = 'system',
    storageKey = 'theme',
    label = 'Apariencia',
    description,
    allowSystem = true,
    className,
    onModeChange,
}) => {
    const [mode, setMode] = React.useState<ThemeMode>(defaultMode);
    const [theme, setTheme] = React.useState<ThemeName>(() => resolveTheme(defaultMode));

    const applyTheme = React.useCallback((nextTheme: ThemeName) => {
        if (!isBrowser) return;
        const root = document.documentElement;
        root.classList.toggle('dark', nextTheme === 'dark');
        root.style.colorScheme = nextTheme;
        root.dataset.theme = nextTheme;
    }, []);

    // Lee la preferencia almacenada o la del sistema al montar
    React.useEffect(() => {
        if (!isBrowser) return;

        const storedMode = storageKey ? localStorage.getItem(storageKey) : null;
        const initialMode = isValidMode(storedMode) ? storedMode : defaultMode;
        const initialTheme = resolveTheme(initialMode);

        setMode(initialMode);
        setTheme(initialTheme);
        applyTheme(initialTheme);
    }, [applyTheme, defaultMode, storageKey]);

    // Actualiza la UI si cambia la preferencia del sistema y seguimos en modo "system"
    React.useEffect(() => {
        if (!isBrowser) return;
        const media = window.matchMedia('(prefers-color-scheme: dark)');

        const handleChange = (event: MediaQueryListEvent) => {
            if (mode !== 'system') return;
            const nextTheme = event.matches ? 'dark' : 'light';
            setTheme(nextTheme);
            applyTheme(nextTheme);
            onModeChange?.('system', nextTheme);
        };

        media.addEventListener('change', handleChange);
        return () => media.removeEventListener('change', handleChange);
    }, [applyTheme, mode, onModeChange]);

    const setAndPersist = (nextMode: ThemeMode) => {
        const nextTheme = resolveTheme(nextMode);
        setMode(nextMode);
        setTheme(nextTheme);
        applyTheme(nextTheme);
        if (storageKey) localStorage.setItem(storageKey, nextMode);
        onModeChange?.(nextMode, nextTheme);
    };

    const handleToggle = (checked: boolean) => setAndPersist(checked ? 'dark' : 'light');
    const handleAuto = () => setAndPersist('system');

    const statusText = description
        ? description
        : mode === 'system'
        ? 'Siguiendo la preferencia del sistema'
        : theme === 'dark'
        ? 'Modo oscuro activo'
        : 'Modo claro activo';

    return (
        <div
            className={cn(
                'flex w-full items-center justify-between gap-4 rounded-2xl border border-border-primary bg-surface-secondary/80 px-4 py-3 shadow-sm backdrop-blur-sm',
                className
            )}
        >
            <div className="flex items-center gap-3">
                <div className="relative flex h-11 w-11 items-center justify-center rounded-xl border border-border-secondary bg-gradient-to-br from-accent-blue/10 via-surface-primary to-accent-blue/30 text-accent-blue shadow-xs">
                    <AnimatePresence mode="wait" initial={false}>
                        <motion.span
                            key={theme}
                            className="flex items-center justify-center"
                            initial={{ opacity: 0, scale: 0.9 }}
                            animate={{ opacity: 1, scale: 1 }}
                            exit={{ opacity: 0, scale: 0.9 }}
                            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                        >
                            {theme === 'dark' ? <Moon className="h-5 w-5" /> : <Sun className="h-5 w-5" />}
                        </motion.span>
                    </AnimatePresence>
                </div>
                <div className="space-y-0.5">
                    <p className="text-sm font-semibold text-text-primary">{label}</p>
                    <p className="text-xs text-text-secondary">{statusText}</p>
                </div>
            </div>

            <div className="flex items-center gap-2">
                {allowSystem && (
                    <button
                        type="button"
                        onClick={handleAuto}
                        className={cn(
                            'group inline-flex items-center gap-1.5 rounded-full border border-border-secondary px-3 py-1 text-xs font-medium text-text-secondary transition-apple hover:border-border-primary hover:text-text-primary',
                            mode === 'system' && 'bg-surface-primary text-text-primary shadow-xs'
                        )}
                    >
                        <Monitor className="h-3.5 w-3.5" />
                        Auto
                    </button>
                )}
                <Switch checked={theme === 'dark'} onCheckedChange={handleToggle} />
            </div>
        </div>
    );
};

/* ========================================
   USAGE
   ======================================== */

/*
<ThemeToggle />

<ThemeToggle
  label="Tema"
  description="Sincroniza con el modo del sistema"
  onModeChange={(mode, theme) => console.log({ mode, theme })}
/>
*/
