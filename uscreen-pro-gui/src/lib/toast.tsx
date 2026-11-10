/**
 * Notifications via shadcn's sonner Toaster. `toast.*` keeps the call shape the app already used.
 * Messages are lifted above the 40px status bar so they never cover the version number.
 */
import { toast as sonner } from "sonner";
import { Toaster } from "@/components/ui/sonner";

/** Options of a toast; `id` replaces an existing toast with the same id. */
type Opts = { id?: string };

/** Thin wrapper over sonner keeping the call shape the app uses; errors stay up longer than other toasts. */
export const toast = {
  success: (message: string, opts?: Opts) => void sonner.success(message, opts),
  info: (message: string) => void sonner(message),
  error: (message: string, opts?: Opts) => void sonner.error(message, { ...opts, duration: 8000 }),
  loading: (message: string, opts?: Opts) => void sonner.loading(message, opts),
};

/** Mount point of the toast stack; place once near the root. */
export function Notifications({ dark }: { dark: boolean }) {
  return <Toaster theme={dark ? "dark" : "light"} position="bottom-right" offset={48} />;
}
