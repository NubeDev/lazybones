import type { ReactNode } from "react";
import * as ScrollAreaPrimitive from "@radix-ui/react-scroll-area";
import { cn } from "@/lib/utils/cn";

/** A themed scroll container with an overlay scrollbar.
 *
 *  The height constraint (`max-h-*`, `h-*`, or `flex-1`) is applied to **both**
 *  the Root and the Viewport. Radix sizes the Viewport to `height: 100%`, which
 *  only resolves against a parent with a concrete height — so the cap has to be
 *  on the Root for the Viewport's `100%` to mean anything, *and* on the Viewport
 *  itself (`max-h`/`flex-1` don't inherit through `height: 100%`). With the cap
 *  only on the Root (the previous behaviour) the Viewport grew with its content
 *  and nothing ever scrolled. Visual styling (`border`, `rounded`, `bg-*`) in the
 *  same `className` lands on both too, which is harmless — the Viewport fills the
 *  Root exactly. */
export function ScrollArea({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <ScrollAreaPrimitive.Root
      className={cn("relative overflow-hidden", className)}
    >
      <ScrollAreaPrimitive.Viewport
        className={cn("max-h-full w-full rounded-[inherit]", className)}
      >
        {children}
      </ScrollAreaPrimitive.Viewport>
      <ScrollAreaPrimitive.Scrollbar
        orientation="vertical"
        className="flex w-2 touch-none select-none p-0.5"
      >
        <ScrollAreaPrimitive.Thumb className="flex-1 rounded-full bg-border-strong" />
      </ScrollAreaPrimitive.Scrollbar>
    </ScrollAreaPrimitive.Root>
  );
}
