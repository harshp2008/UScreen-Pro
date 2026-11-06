import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconButton } from "@bug-on/m3-expressive/buttons";
import { Minus, Moon, Square, Sun, X, Copy } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import appIcon from "../assets/uscreen-pro.svg";
import { cx } from "./ui/controls";

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
    <IconButton
      aria-label={label}
      title={label}
      onClick={onClick}
      size="xs"
      colorStyle="standard"
      className={cx("text-on-surface-variant", danger && "hover:text-error")}
    >
      {children}
    </IconButton>
  );
}

/**
 * Custom frameless title bar. `data-tauri-drag-region` only fires on the element it is set on,
 * so the label is pointer-events-none and drags fall through to the bar itself.
 */
export function TitleBar({ dark, onToggleTheme }: { dark: boolean; onToggleTheme: () => void }) {
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
        <span className="text-xs font-semibold text-on-surface">UScreen Pro</span>
      </div>
      <div className="flex items-center gap-1">
        <WindowButton label={dark ? "Switch to light mode" : "Switch to dark mode"} onClick={onToggleTheme}>
          {dark ? <Sun className="size-4" /> : <Moon className="size-4" />}
        </WindowButton>
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
