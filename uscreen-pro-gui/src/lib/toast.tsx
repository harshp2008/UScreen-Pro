/**
 * Notifications via shadcn's sonner Toaster. `toast.*` keeps the call shape the app already used.
 * Messages are lifted above the 40px status bar so they never cover the version number.
 */
import { toast as sonner } from "sonner";
import { Toaster } from "@/components/ui/sonner";

type Opts = { id?: string };

export const toast = {
  success: (message: string, opts?: Opts) => void sonner.success(message, opts),
  info: (message: string) => void sonner(message),
  error: (message: string, opts?: Opts) => void sonner.error(message, { ...opts, duration: 8000 }),
  loading: (message: string, opts?: Opts) => void sonner.loading(message, opts),
};

export function Notifications({ dark }: { dark: boolean }) {
  return <Toaster theme={dark ? "dark" : "light"} position="bottom-right" offset={48} />;
}
