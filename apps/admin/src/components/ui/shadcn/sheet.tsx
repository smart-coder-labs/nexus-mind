"use client";

import * as SheetPrimitive from "@radix-ui/react-dialog";
import { XIcon } from "lucide-react";
import type * as React from "react";

import { cn } from "@/lib/utils";

function Sheet({ ...props }: React.ComponentProps<typeof SheetPrimitive.Root>): React.JSX.Element {
	return <SheetPrimitive.Root data-slot="sheet" {...props} />;
}

function SheetTrigger({
	...props
}: React.ComponentProps<typeof SheetPrimitive.Trigger>): React.JSX.Element {
	return <SheetPrimitive.Trigger data-slot="sheet-trigger" {...props} />;
}

function SheetClose({
	...props
}: React.ComponentProps<typeof SheetPrimitive.Close>): React.JSX.Element {
	return <SheetPrimitive.Close data-slot="sheet-close" {...props} />;
}

function SheetPortal({
	...props
}: React.ComponentProps<typeof SheetPrimitive.Portal>): React.JSX.Element {
	return <SheetPrimitive.Portal data-slot="sheet-portal" {...props} />;
}

function SheetOverlay({
	className,
	...props
}: React.ComponentProps<typeof SheetPrimitive.Overlay>): React.JSX.Element {
	return (
		<SheetPrimitive.Overlay
			data-slot="sheet-overlay"
			className={cn(
				"fixed inset-0 z-50 bg-black/45 backdrop-blur-[2px] data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 duration-200",
				className,
			)}
			{...props}
		/>
	);
}

function SheetContent({
	className,
	children,
	side = "right",
	...props
}: React.ComponentProps<typeof SheetPrimitive.Content> & {
	side?: "top" | "right" | "bottom" | "left";
}): React.JSX.Element {
	return (
		<SheetPortal>
			<SheetOverlay />
			<SheetPrimitive.Content
				data-slot="sheet-content"
				className={cn(
				"bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out fixed z-50 flex min-h-0 flex-col gap-0 border-border shadow-[0_24px_80px_-24px_rgba(0,0,0,.48),0_8px_24px_-12px_rgba(0,0,0,.18)] transition ease-[cubic-bezier(.16,1,.3,1)] data-[state=closed]:duration-200 data-[state=open]:duration-300",
					side === "right" &&
					"data-[state=closed]:slide-out-to-right data-[state=open]:slide-in-from-right inset-y-0 right-0 h-dvh w-[min(24rem,calc(100%-1rem))] rounded-l-2xl border-y-0 border-r-0 border-l sm:max-w-sm",
					side === "left" &&
					"data-[state=closed]:slide-out-to-left data-[state=open]:slide-in-from-left inset-y-0 left-0 h-dvh w-[min(24rem,calc(100%-1rem))] rounded-r-2xl border-y-0 border-l-0 border-r sm:max-w-sm",
					side === "top" &&
					"data-[state=closed]:slide-out-to-top data-[state=open]:slide-in-from-top inset-x-0 top-0 h-auto rounded-b-2xl border-x-0 border-t-0 border-b",
					side === "bottom" &&
					"data-[state=closed]:slide-out-to-bottom data-[state=open]:slide-in-from-bottom inset-x-0 bottom-0 h-auto rounded-t-2xl border-x-0 border-b-0 border-t",
					className,
				)}
				{...props}
			>
				{children}
				<SheetPrimitive.Close className="ring-offset-background focus-visible:ring-ring data-[state=open]:bg-secondary absolute right-5 top-5 z-10 grid size-8 cursor-pointer place-items-center rounded-lg border border-transparent text-muted-foreground transition-colors hover:border-border hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-offset-2 focus:outline-hidden disabled:pointer-events-none">
					<XIcon className="size-4" />
					<span className="sr-only">Close</span>
				</SheetPrimitive.Close>
			</SheetPrimitive.Content>
		</SheetPortal>
	);
}

function SheetHeader({ className, ...props }: React.ComponentProps<"div">): React.JSX.Element {
	return (
		<div
			data-slot="sheet-header"
			className={cn("flex shrink-0 flex-col gap-1.5 border-b border-border/70 px-6 pb-4 pt-6 pr-16", className)}
			{...props}
		/>
	);
}

function SheetFooter({ className, ...props }: React.ComponentProps<"div">): React.JSX.Element {
	return (
		<div
			data-slot="sheet-footer"
			className={cn("mt-auto flex shrink-0 flex-col gap-2 border-t border-border/70 px-6 py-4 pb-[max(1rem,env(safe-area-inset-bottom))]", className)}
			{...props}
		/>
	);
}

function SheetTitle({
	className,
	...props
}: React.ComponentProps<typeof SheetPrimitive.Title>): React.JSX.Element {
	return (
		<SheetPrimitive.Title
			data-slot="sheet-title"
			className={cn("text-foreground font-semibold", className)}
			{...props}
		/>
	);
}

function SheetDescription({
	className,
	...props
}: React.ComponentProps<typeof SheetPrimitive.Description>): React.JSX.Element {
	return (
		<SheetPrimitive.Description
			data-slot="sheet-description"
			className={cn("text-muted-foreground text-sm", className)}
			{...props}
		/>
	);
}

export {
	Sheet,
	SheetTrigger,
	SheetClose,
	SheetContent,
	SheetHeader,
	SheetFooter,
	SheetTitle,
	SheetDescription,
};
