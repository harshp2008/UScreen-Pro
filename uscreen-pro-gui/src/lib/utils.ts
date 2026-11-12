/*
 * Small shared helpers: `cn()` joins class names and resolves Tailwind conflicts (clsx + tailwind-merge); `errText()` turns a thrown value into text.
 */
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** A readable message from a thrown value (an `Error`'s message, otherwise the value as text). */
export const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Merge conditional class names, letting later Tailwind utilities win over earlier conflicting ones. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
