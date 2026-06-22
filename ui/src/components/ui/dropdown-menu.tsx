import type { ComponentPropsWithoutRef, ReactNode } from "react";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { cn } from "@/lib/utils/cn";

export const DropdownMenu = DropdownPrimitive.Root;
export const DropdownMenuTrigger = DropdownPrimitive.Trigger;

/** The floating menu panel. Styled to match the dialog surface. */
export function DropdownMenuContent({
  children,
  className,
  align = "end",
  sideOffset = 6,
}: {
  children: ReactNode;
  className?: string;
  align?: DropdownPrimitive.DropdownMenuContentProps["align"];
  sideOffset?: number;
}) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.Content
        align={align}
        sideOffset={sideOffset}
        className={cn(
          "z-50 min-w-44 overflow-hidden rounded-lg border border-border bg-surface p-1 shadow-2xl",
          "data-[state=open]:animate-fade-up",
          className,
        )}
      >
        {children}
      </DropdownPrimitive.Content>
    </DropdownPrimitive.Portal>
  );
}

/** A single menu row. `tone="danger"` renders it in the blocked/destructive color. */
export function DropdownMenuItem({
  children,
  className,
  tone = "default",
  ...props
}: ComponentPropsWithoutRef<typeof DropdownPrimitive.Item> & {
  tone?: "default" | "danger";
}) {
  return (
    <DropdownPrimitive.Item
      className={cn(
        "flex w-full cursor-pointer select-none items-center gap-2 rounded-md px-2.5 py-2 text-xs outline-none",
        "[&_svg]:size-3.5 [&_svg]:shrink-0",
        "data-[disabled]:pointer-events-none data-[disabled]:opacity-40",
        tone === "danger"
          ? "text-status-blocked data-[highlighted]:bg-status-blocked/15"
          : "text-foreground data-[highlighted]:bg-muted",
        className,
      )}
      {...props}
    >
      {children}
    </DropdownPrimitive.Item>
  );
}

export function DropdownMenuSeparator() {
  return <DropdownPrimitive.Separator className="my-1 h-px bg-border" />;
}

export function DropdownMenuLabel({ children }: { children: ReactNode }) {
  return (
    <DropdownPrimitive.Label className="px-2.5 py-1.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
      {children}
    </DropdownPrimitive.Label>
  );
}
