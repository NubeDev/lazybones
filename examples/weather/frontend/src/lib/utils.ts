import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Merge conditional class lists, resolving Tailwind conflicts last-wins.
 *  Bundled into the remote (the host's `cn` is not a shared singleton); behaves
 *  identically and uses the host's served Tailwind tokens. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
