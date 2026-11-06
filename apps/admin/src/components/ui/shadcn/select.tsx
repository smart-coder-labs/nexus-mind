"use client";

import * as SelectPrimitive from "@radix-ui/react-select";
import { Activity, Archive, ArrowDown, ArrowUp, Ban, BadgeCheck, BookOpen, Brain, Bug, CalendarRange, CheckIcon, Circle, CircleDashed, CircleOff, ChevronDownIcon, ChevronUpIcon, Code2, Compass, Component, Database, Eye, FileCode2, FileText, Folder, Hash, Lightbulb, ListChecks, ListTodo, LoaderCircle, Palette, Rocket, Search, Settings2, Siren, Tag, UserRound, Users, Wrench } from "lucide-react";
import type * as React from "react";

import { cn } from "@/lib/utils";

const OPTION_PRESENTATION: Record<string, { description: string; color: string; icon: React.ReactNode }> = {
	explore: { description: "Gather context, constraints, and the problem to solve.", color: "var(--data-blue)", icon: <Activity /> },
	propose: { description: "Compare approaches and agree on a direction.", color: "var(--data-amber)", icon: <Lightbulb /> },
	spec: { description: "Define requirements and acceptance criteria.", color: "var(--data-blue)", icon: <FileText /> },
	design: { description: "Plan the architecture, interfaces, and experience.", color: "var(--chart-5)", icon: <Palette /> },
	tasks: { description: "Break approved changes into actionable work.", color: "var(--data-teal)", icon: <ListChecks /> },
	apply: { description: "Implement the planned change.", color: "var(--color-status-info)", icon: <Rocket /> },
	verify: { description: "Run checks and confirm the expected behavior.", color: "var(--color-status-success)", icon: <BadgeCheck /> },
	archive: { description: "Preserve completed work for reference.", color: "var(--muted-foreground)", icon: <Archive /> },
	active: { description: "Currently in use.", color: "var(--color-status-success)", icon: <Activity /> },
	paused: { description: "Temporarily on hold.", color: "var(--color-status-warning)", icon: <CircleDashed /> },
	archived: { description: "Completed and kept for reference.", color: "var(--muted-foreground)", icon: <Archive /> },
	abandoned: { description: "Stopped before completion.", color: "var(--color-status-warning)", icon: <CircleOff /> },
	backlog: { description: "Captured for later; work has not started.", color: "var(--muted-foreground)", icon: <ListTodo /> },
	todo: { description: "Ready to be picked up.", color: "var(--color-status-info)", icon: <Circle /> },
	"in progress": { description: "Someone is actively working on it.", color: "var(--brand-link)", icon: <LoaderCircle /> },
	"in review": { description: "Waiting for review or approval.", color: "var(--color-status-warning)", icon: <Eye /> },
	done: { description: "Completed and verified.", color: "var(--color-status-success)", icon: <BadgeCheck /> },
	cancelled: { description: "Stopped and will not be completed.", color: "var(--color-status-error)", icon: <Ban /> },
	low: { description: "Can wait until higher-priority work is done.", color: "var(--muted-foreground)", icon: <ArrowDown /> },
	medium: { description: "Normal priority for planned work.", color: "var(--color-status-info)", icon: <Activity /> },
	high: { description: "Should be addressed soon.", color: "var(--color-status-warning)", icon: <ArrowUp /> },
	urgent: { description: "Needs immediate attention.", color: "var(--color-status-error)", icon: <Siren /> },
	pending: { description: "Waiting to start.", color: "var(--color-status-warning)", icon: <CircleDashed /> },
	running: { description: "In progress now.", color: "var(--color-status-info)", icon: <LoaderCircle /> },
	completed: { description: "Finished successfully.", color: "var(--color-status-success)", icon: <BadgeCheck /> },
	failed: { description: "Needs attention before it can continue.", color: "var(--color-status-error)", icon: <CircleOff /> },
	decision: { description: "A choice the team agreed to keep as guidance.", color: "var(--data-blue)", icon: <Lightbulb /> },
	bugfix: { description: "A repair for incorrect or broken behavior.", color: "var(--color-status-error)", icon: <Bug /> },
	discovery: { description: "A finding that improves understanding of the work.", color: "var(--chart-5)", icon: <Compass /> },
	feature: { description: "A new capability or user-facing improvement.", color: "var(--data-teal)", icon: <Rocket /> },
	architecture: { description: "A structural design or system boundary.", color: "var(--data-blue)", icon: <Component /> },
	config: { description: "A setting or environment-specific choice.", color: "var(--data-amber)", icon: <Settings2 /> },
	preference: { description: "A user or team preference to remember.", color: "var(--chart-5)", icon: <UserRound /> },
	pattern: { description: "A reusable approach observed in the project.", color: "var(--data-teal)", icon: <Component /> },
	convention: { description: "A rule followed consistently across the project.", color: "var(--color-status-success)", icon: <BookOpen /> },
	project: { description: "Limit this view to project-scoped information.", color: "var(--data-blue)", icon: <Folder /> },
	personal: { description: "Information scoped to your personal workspace.", color: "var(--chart-5)", icon: <UserRound /> },
	"all types": { description: "Include every memory category.", color: "var(--muted-foreground)", icon: <Brain /> },
	"all scopes": { description: "Include project and personal memories.", color: "var(--muted-foreground)", icon: <Users /> },
	"all projects": { description: "Show results from every project.", color: "var(--muted-foreground)", icon: <Folder /> },
	"all collections": { description: "Show memories with any collection assignment.", color: "var(--muted-foreground)", icon: <Folder /> },
	"newest first": { description: "Sort by most recently created.", color: "var(--data-blue)", icon: <Activity /> },
	"oldest first": { description: "Sort by earliest created.", color: "var(--muted-foreground)", icon: <Archive /> },
	"most revised": { description: "Sort by revision count.", color: "var(--data-teal)", icon: <FileCode2 /> },
	"all users": { description: "Include activity from every user.", color: "var(--muted-foreground)", icon: <Users /> },
	"all actions": { description: "Include every audit event type.", color: "var(--muted-foreground)", icon: <Activity /> },
	"all resources": { description: "Include every resource category.", color: "var(--muted-foreground)", icon: <Database /> },
	"all assignees": { description: "Show tasks assigned to anyone.", color: "var(--muted-foreground)", icon: <Users /> },
	"all statuses": { description: "Include every status.", color: "var(--muted-foreground)", icon: <ListChecks /> },
	"all priorities": { description: "Include every priority.", color: "var(--muted-foreground)", icon: <Activity /> },
	"all targets": { description: "Include every compatible target.", color: "var(--muted-foreground)", icon: <Component /> },
	"all owners": { description: "Include harnesses from every owner.", color: "var(--muted-foreground)", icon: <Users /> },
	"all clients": { description: "Show projects for every client.", color: "var(--muted-foreground)", icon: <Folder /> },
	"all time": { description: "Include results from every time period.", color: "var(--muted-foreground)", icon: <Activity /> },
	"no project": { description: "Keep this item outside a project.", color: "var(--muted-foreground)", icon: <Folder /> },
	"no sprint": { description: "Leave this change outside a sprint.", color: "var(--muted-foreground)", icon: <CalendarRange /> },
	"claude": { description: "Run with the Claude model family.", color: "var(--chart-5)", icon: <Brain /> },
	"codex": { description: "Run with the OpenAI Codex model family.", color: "var(--data-blue)", icon: <Code2 /> },
	"cursor": { description: "Use the Cursor development environment.", color: "var(--data-teal)", icon: <Component /> },
	"playwright": { description: "Use browser-driven interaction and verification.", color: "var(--data-blue)", icon: <Search /> },
	"manual": { description: "Run only when a person starts it.", color: "var(--muted-foreground)", icon: <UserRound /> },
	"minutes": { description: "Use a minutes-based interval.", color: "var(--data-amber)", icon: <Activity /> },
	"hours": { description: "Use an hours-based interval.", color: "var(--data-blue)", icon: <Activity /> },
	"days": { description: "Use a days-based interval.", color: "var(--chart-5)", icon: <Activity /> },
	"claude pure": { description: "Connect directly to Claude Code.", color: "var(--chart-5)", icon: <Brain /> },
	"nexus harness (openshell)": { description: "Run through the NexusMind managed harness.", color: "var(--data-lime)", icon: <Wrench /> },
	"tags": { description: "Memories with a tag assigned.", color: "var(--data-amber)", icon: <Tag /> },
	"hash": { description: "Filter by a tag name.", color: "var(--data-amber)", icon: <Hash /> },
};

function Select({
	...props
}: React.ComponentProps<typeof SelectPrimitive.Root>): React.JSX.Element {
	return <SelectPrimitive.Root data-slot="select" {...props} />;
}

function SelectGroup({
	...props
}: React.ComponentProps<typeof SelectPrimitive.Group>): React.JSX.Element {
	return <SelectPrimitive.Group data-slot="select-group" {...props} />;
}

function SelectValue({
	...props
}: React.ComponentProps<typeof SelectPrimitive.Value>): React.JSX.Element {
	return <SelectPrimitive.Value data-slot="select-value" {...props} />;
}

function SelectTrigger({
	className,
	size = "default",
	children,
	...props
}: React.ComponentProps<typeof SelectPrimitive.Trigger> & {
	size?: "sm" | "default";
}): React.JSX.Element {
	return (
		<SelectPrimitive.Trigger
			data-slot="select-trigger"
			data-size={size}
			className={cn(
				"border-input data-placeholder:text-muted-foreground [&_svg:not([class*='text-'])]:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 data-[state=open]:border-ring data-[state=open]:ring-[3px] data-[state=open]:ring-ring/20 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:bg-input/30 dark:hover:bg-input/50 flex w-fit items-center justify-between gap-2 rounded-md border bg-transparent px-3 py-2 text-sm whitespace-nowrap shadow-xs transition-[color,background-color,border-color,box-shadow] outline-none focus-visible:ring-[3px] disabled:cursor-not-allowed disabled:opacity-50 data-[size=default]:h-9 data-[size=sm]:h-8 *:data-[slot=select-value]:line-clamp-1 *:data-[slot=select-value]:flex *:data-[slot=select-value]:items-center *:data-[slot=select-value]:gap-2 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 cursor-pointer",
				className,
			)}
			{...props}
		>
			{children}
			<SelectPrimitive.Icon asChild>
				<ChevronDownIcon className="size-4 opacity-50" />
			</SelectPrimitive.Icon>
		</SelectPrimitive.Trigger>
	);
}

function SelectContent({
	className,
	children,
	position = "popper",
	align = "center",
	...props
}: React.ComponentProps<typeof SelectPrimitive.Content>): React.JSX.Element {
	return (
		<SelectPrimitive.Portal>
			<SelectPrimitive.Content
				data-slot="select-content"
				className={cn(
					"border-border bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 relative z-50 max-h-(--radix-select-content-available-height) min-w-32 max-w-[min(26rem,calc(100vw-2rem))] origin-(--radix-select-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-lg border shadow-lg shadow-black/10 dark:shadow-black/30",
					position === "popper" &&
						"data-[side=bottom]:translate-y-1 data-[side=left]:-translate-x-1 data-[side=right]:translate-x-1 data-[side=top]:-translate-y-1",
					className,
				)}
				position={position}
				align={align}
				{...props}
			>
				<SelectScrollUpButton />
				<SelectPrimitive.Viewport
					className={cn(
						"p-1",
						position === "popper" &&
							"h-(--radix-select-trigger-height) w-full min-w-(--radix-select-trigger-width) scroll-my-1",
					)}
				>
					{children}
				</SelectPrimitive.Viewport>
				<SelectScrollDownButton />
			</SelectPrimitive.Content>
		</SelectPrimitive.Portal>
	);
}

function SelectLabel({
	className,
	...props
}: React.ComponentProps<typeof SelectPrimitive.Label>): React.JSX.Element {
	return (
		<SelectPrimitive.Label
			data-slot="select-label"
			className={cn("text-muted-foreground px-2 py-1.5 text-xs", className)}
			{...props}
		/>
	);
}

function SelectItem({
	className,
	children,
	description,
	icon,
	avatarSrc,
	indicatorColor,
	...props
}: React.ComponentProps<typeof SelectPrimitive.Item> & {
	description?: React.ReactNode;
	icon?: React.ReactNode;
	avatarSrc?: string;
	indicatorColor?: string;
}): React.JSX.Element {
	const optionKey = typeof children === "string" ? children.trim().toLowerCase().replace(/_/g, " ").replace(/\s*\(\d+\)$/, "") : "";
	const valueKey = String(props.value).replace(/^nexus:/, "").trim().toLowerCase().replace(/_/g, " ");
	const presentation = OPTION_PRESENTATION[optionKey] ?? OPTION_PRESENTATION[valueKey];
	description ??= presentation?.description;
	icon ??= presentation?.icon;
	indicatorColor ??= presentation?.color;
	return (
		<SelectPrimitive.Item
			data-slot="select-item"
			className={cn(
				"focus:bg-accent focus:text-accent-foreground data-[state=checked]:text-foreground relative flex min-h-9 w-full cursor-pointer items-center gap-2 rounded-md py-1.5 pr-8 pl-2 text-sm outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50",
				className,
			)}
			{...props}
		>
			{(icon || avatarSrc || indicatorColor) && (
				<span className="relative grid size-7 shrink-0 place-items-center overflow-hidden rounded-md bg-muted text-muted-foreground [&_svg]:size-3.5" aria-hidden="true">
					{avatarSrc ? <img src={avatarSrc} alt="" className="size-full object-cover" /> : <span className="grid size-full place-items-center" style={indicatorColor ? { color: indicatorColor } : undefined}>{icon}</span>}
					{indicatorColor && !avatarSrc && <span className="absolute bottom-0.5 right-0.5 size-2 rounded-full border-2 border-popover" style={{ backgroundColor: indicatorColor }} />}
				</span>
			)}
			<span className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
				<SelectPrimitive.ItemText className="w-full truncate text-left text-[13px] leading-4">{children}</SelectPrimitive.ItemText>
				{description && <span className="w-full truncate text-left text-[11px] leading-[14px] text-muted-foreground">{description}</span>}
			</span>
			<span className="absolute right-2 grid size-4 place-items-center text-primary">
				<SelectPrimitive.ItemIndicator>
					<CheckIcon className="size-4" />
				</SelectPrimitive.ItemIndicator>
			</span>
		</SelectPrimitive.Item>
	);
}

function SelectSeparator({
	className,
	...props
}: React.ComponentProps<typeof SelectPrimitive.Separator>): React.JSX.Element {
	return (
		<SelectPrimitive.Separator
			data-slot="select-separator"
			className={cn("bg-border pointer-events-none -mx-1 my-1 h-px", className)}
			{...props}
		/>
	);
}

function SelectScrollUpButton({
	className,
	...props
}: React.ComponentProps<typeof SelectPrimitive.ScrollUpButton>): React.JSX.Element {
	return (
		<SelectPrimitive.ScrollUpButton
			data-slot="select-scroll-up-button"
			className={cn(
				"flex cursor-default items-center justify-center py-1",
				className,
			)}
			{...props}
		>
			<ChevronUpIcon className="size-4" />
		</SelectPrimitive.ScrollUpButton>
	);
}

function SelectScrollDownButton({
	className,
	...props
}: React.ComponentProps<typeof SelectPrimitive.ScrollDownButton>): React.JSX.Element {
	return (
		<SelectPrimitive.ScrollDownButton
			data-slot="select-scroll-down-button"
			className={cn(
				"flex cursor-default items-center justify-center py-1",
				className,
			)}
			{...props}
		>
			<ChevronDownIcon className="size-4" />
		</SelectPrimitive.ScrollDownButton>
	);
}

export {
	Select,
	SelectContent,
	SelectGroup,
	SelectItem,
	SelectLabel,
	SelectScrollDownButton,
	SelectScrollUpButton,
	SelectSeparator,
	SelectTrigger,
	SelectValue,
};
