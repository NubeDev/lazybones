import type { ReactNode } from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cn } from "@/lib/utils/cn";

export const TooltipProvider = TooltipPrimitive.Provider;

/** A minimal tooltip wrapper: pass the trigger as children + a `label`. */
export function Tooltip({
  children,
  label,
  side = "bottom",
}: {
  children: ReactNode;
  label: ReactNode;
  side?: "top" | "bottom" | "left" | "right";
}) {
  return (
    <TooltipPrimitive.Root delayDuration={300}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className={cn(
            "z-50 overflow-hidden rounded-md border border-border bg-surface-2 px-2.5 py-1.5",
            "text-xs text-foreground shadow-md",
            "data-[state=delayed-open]:animate-fade-up",
          )}
        >
          {label}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
