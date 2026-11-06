"use client";

import { Slot } from "@radix-ui/react-slot";
import { cva } from "class-variance-authority";
import { LoaderCircleIcon } from "lucide-react";
import type * as React from "react";
import { cn } from "@/lib/utils";

export interface ButtonVariantProps {
	variant?:
		| "default"
		| "destructive"
		| "outline"
		| "secondary"
		| "ghost"
		| "link"
		| null;
	size?: "default" | "sm" | "xs" | "lg" | "icon" | null;
}

const buttonVariants = cva(
	"inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive cursor-pointer",
	{
		variants: {
			variant: {
				default:
					"bg-complementary text-complementary-foreground shadow-xs hover:bg-complementary/90",
				destructive:
					"bg-destructive text-white shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60",
				outline:
					"border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50",
				secondary:
					"bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80",
				ghost:
					"hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
				link: "text-accent-blue underline-offset-4 hover:underline",
			},
			size: {
				default: "h-9 px-4 py-2 has-[>svg]:px-3",
				sm: "h-8 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5",
				xs: "h-5 rounded-sm gap-1.5 px-2 has-[>svg]:px-1 text-xs",
				lg: "h-10 rounded-md px-6 has-[>svg]:px-4",
				icon: "size-9",
			},
		},
		defaultVariants: {
			variant: "default",
			size: "default",
		},
	},
);

function Button({
	className,
	variant,
	size,
	asChild = false,
	disabled,
	children,
	loading = false,
	type = "button",
	forceShowChildren = false,
	classNameLoader,
	...props
}: React.ComponentProps<"button"> &
	ButtonVariantProps & {
		asChild?: boolean;
		loading?: boolean;
		forceShowChildren?: boolean;
		classNameLoader?: string;
	}): React.JSX.Element {
	const Comp = asChild ? Slot : "button";

	return (
		<Comp
			data-slot="button"
			className={cn(
				buttonVariants({ variant, size, className }),
				disabled && "cursor-not-allowed",
			)}
			disabled={disabled || loading}
			type={type}
			{...props}
		>
			{loading ? (
				<>
					<LoaderCircleIcon className={cn("animate-spin", classNameLoader)} />
					{forceShowChildren ? children : null}
				</>
			) : (
				children
			)}
		</Comp>
	);
}

export { Button, buttonVariants };
