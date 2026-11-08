import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Copy, Minus, Monitor, Moon, Square, Sun, X, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ThemePref } from "@/lib/theme";
import appIcon from "../assets/uscreen-pro.svg";
import { cn } from "@/lib/utils";

function WindowButton({
  label,
  onClick,
  danger,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: ReactNode;
}) {
  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn("size-7 text-muted-foreground", danger && "hover:bg-destructive/15 hover:text-destructive")}
    >
      {children}
    </Button>
  );
}

/**
 * Custom frameless title bar. `data-tauri-drag-region` only fires on the element it is set on,
 * so the label is pointer-events-none and drags fall through to the bar itself.
 */
const THEMES: { id: ThemePref; label: string; icon: LucideIcon }[] = [
  { id: "light", label: "Light", icon: Sun },
  { id: "dark", label: "Dark", icon: Moon },
  { id: "system", label: "System (Auto)", icon: Monitor },
];

/** Custom window title bar (the native decorations are off) with drag region, window controls and the theme picker. */
export function TitleBar({ theme, onThemeChange }: { theme: ThemePref; onThemeChange: (t: ThemePref) => void }) {
  const current = THEMES.find((t) => t.id === theme) ?? THEMES[2];
  const CurrentIcon = current.icon;
  const win = useMemo(() => {
    try {
      return getCurrentWindow();
    } catch {
      return null; // not running inside Tauri (plain browser preview)
    }
  }, []);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!win) return;
    const sync = () => void win.isMaximized().then((m) => setMaximized(Boolean(m)));
    sync();
    const unlisten = win.onResized(sync);
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [win]);

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 select-none items-center justify-between bg-transparent pl-4 pr-2"
    >
      <div className="pointer-events-none flex items-center gap-2">
        <img src={appIcon} alt="" className="size-5" draggable={false} />
        <span className="text-xs font-semibold text-foreground">UScreen Pro</span>
      </div>
      <div className="flex items-center gap-1">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              aria-label={`Theme: ${current.label}`}
              title={`Theme: ${current.label}`}
              className="size-7 text-muted-foreground"
            >
              <CurrentIcon className="size-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuRadioGroup value={theme} onValueChange={(v) => onThemeChange(v as ThemePref)}>
              {THEMES.map(({ id, label, icon: Icon }) => (
                <DropdownMenuRadioItem key={id} value={id}>
                  <Icon className="mr-2 size-4" />
                  {label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <WindowButton label="Minimize" onClick={() => void win?.minimize()}>
          <Minus className="size-4" />
        </WindowButton>
        <WindowButton label={maximized ? "Restore" : "Maximize"} onClick={() => void win?.toggleMaximize()}>
          {maximized ? <Copy className="size-3.5" /> : <Square className="size-3.5" />}
        </WindowButton>
        <WindowButton label="Close" danger onClick={() => void win?.close()}>
          <X className="size-4" />
        </WindowButton>
      </div>
    </header>
  );
}
