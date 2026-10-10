import * as React from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface ModalProps {
  open: boolean;
  ariaLabel?: string;
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
  size?: 'sm' | 'md' | 'lg' | 'xl' | '2xl' | 'full';
  position?: 'center' | 'right' | 'left' | 'bottom' | 'fullscreen';
}

const sizes = {
  sm: 'max-w-sm',
  md: 'max-w-md',
  lg: 'max-w-lg',
  xl: 'max-w-xl',
  '2xl': 'max-w-2xl',
  full: 'max-w-full',
};

const positions = {
  center: 'left-1/2 top-1/2 max-h-[min(90dvh,52rem)] w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 rounded-2xl sm:w-[calc(100%-3rem)]',
  right: 'inset-y-0 right-0 h-dvh w-[min(44rem,calc(100%-1rem))] rounded-l-2xl',
  left: 'inset-y-0 left-0 h-dvh w-[min(44rem,calc(100%-1rem))] rounded-r-2xl',
  bottom: 'bottom-0 left-1/2 max-h-[90dvh] w-full -translate-x-1/2 rounded-t-2xl',
  fullscreen: 'inset-0 h-dvh w-full rounded-none',
};

export function Modal({ open, ariaLabel, onOpenChange, children, size = 'md', position = 'center' }: ModalProps) {
  const opener = React.useRef<HTMLElement | null>(null);
  const [content, setContent] = React.useState<HTMLDivElement | null>(null);
  const fallbackId = React.useId();
  const [title, setTitle] = React.useState<string>();

  React.useLayoutEffect(() => {
    if (!content) return;
    const heading = content.querySelector<HTMLElement>('h1,h2,h3,[data-slot="dialog-title"]');
    if (heading) {
      if (!heading.id) heading.id = fallbackId;
      setTitle(heading.id);
    }
  }, [content, fallbackId]);

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay
          data-slot="dialog-overlay"
          className="fixed inset-0 z-50 bg-black/45 backdrop-blur-[2px] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0 duration-200"
        />
        <Dialog.Content
          ref={setContent}
          data-slot="dialog-content"
          data-position={position}
          aria-modal="true"
          aria-label={ariaLabel}
          aria-labelledby={ariaLabel ? undefined : title}
          aria-describedby={undefined}
          onOpenAutoFocus={() => { opener.current = document.activeElement as HTMLElement; }}
          onCloseAutoFocus={event => {
            if (opener.current?.isConnected) {
              event.preventDefault();
              opener.current.focus();
            }
          }}
          className={cn(
            'admin-dialog fixed z-50 flex min-h-0 flex-col overflow-hidden border border-border bg-popover text-popover-foreground shadow-[0_24px_80px_-24px_rgba(0,0,0,.48),0_8px_24px_-12px_rgba(0,0,0,.18)] outline-none duration-200 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=open]:fade-in-0 data-[state=closed]:fade-out-0 data-[state=open]:zoom-in-[.98] data-[state=closed]:zoom-out-[.98]',
            position === 'center' && 'p-6 sm:p-7',
            position === 'right' && 'border-y-0 border-r-0 p-6 sm:p-8',
            position === 'left' && 'border-y-0 border-l-0 p-6 sm:p-8',
            position === 'bottom' && 'border-x-0 border-b-0 p-6 pb-[max(1.5rem,env(safe-area-inset-bottom))]',
            position === 'fullscreen' && 'p-6 sm:p-8',
            sizes[size],
            positions[position],
          )}
        >
          {ariaLabel && <Dialog.Title className="sr-only">{ariaLabel}</Dialog.Title>}
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export const ModalHeader = ({ children, className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div data-slot="dialog-header" className={cn('admin-dialog-header mb-5 flex shrink-0 flex-col gap-1.5 border-b border-border/70 pb-4 pr-9', className)} {...props}>
    {children}
  </div>
);

export const ModalTitle = ({ children, className, ...props }: React.ComponentProps<typeof Dialog.Title>) => (
  <Dialog.Title data-slot="dialog-title" className={cn('text-lg font-semibold tracking-tight leading-snug text-foreground', className)} {...props}>{children}</Dialog.Title>
);

export const ModalDescription = ({ children, className, ...props }: React.ComponentProps<typeof Dialog.Description>) => (
  <Dialog.Description data-slot="dialog-description" className={cn('text-sm leading-relaxed text-muted-foreground', className)} {...props}>{children}</Dialog.Description>
);

export const ModalContent = ({ children, className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div data-slot="dialog-body" className={cn('admin-dialog-body min-h-0 py-1', className)} {...props}>{children}</div>
);

export const ModalFooter = ({ children, className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div data-slot="dialog-footer" className={cn('admin-dialog-footer mt-5 flex shrink-0 flex-wrap justify-end gap-2 border-t border-border/70 pt-4', className)} {...props}>{children}</div>
);

export const ModalClose = ({ children, className, ...props }: React.ComponentProps<typeof Dialog.Close>) => (
  <Dialog.Close className={className} {...props}>{children}</Dialog.Close>
);

export const ModalCloseButton = ({ className }: { className?: string }) => (
  <Dialog.Close
    aria-label="Close"
    data-slot="dialog-close"
    className={cn('absolute right-4 top-4 z-10 grid size-8 place-items-center rounded-lg border border-transparent text-muted-foreground outline-none transition-colors hover:border-border hover:bg-muted hover:text-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/40 disabled:pointer-events-none', className)}
  >
    <X className="size-4" />
  </Dialog.Close>
);
