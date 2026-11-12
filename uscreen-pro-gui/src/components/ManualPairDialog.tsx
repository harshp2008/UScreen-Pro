/**
 * ManualPairDialog.tsx: the "Manual Setup" dialog that pairs a device which is not plugged in.
 * Input: the pairing IP:port and 6-digit code typed by the user; output: a Tauri `invoke` of the pairing command.
 * Does not own the guided wizard (AddDeviceWizard.tsx).
 */
import { invoke } from "@tauri-apps/api/core";
import { KeyRound, LoaderCircle, Wifi } from "lucide-react";
import { useState } from "react";
import type { ManualPairResult } from "../lib/types";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { errText } from "@/lib/utils";
import { TextInput } from "./ui/controls";

/**
 * Manual Setup: pair a device that is not plugged in, with the pairing IP:port and 6-digit code from
 * its Wireless debugging screen. A dialog of its own (not part of the guide accordion) so the three
 * inputs have room and the guide stays a short read. Mounted only while open, so it starts empty.
 */
export function ManualPairDialog({
  onClose,
  onPaired,
}: {
  onClose: () => void;
  /** The `ip:port` adb accepted; the wizard takes it from here. */
  onPaired: (connectEndpoint: string) => void | Promise<void>;
}) {
  const [pairAddr, setPairAddr] = useState("");
  const [pairCode, setPairCode] = useState("");
  const [connectAddr, setConnectAddr] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /**
   * Pairs with the typed address and code and connects to the connect address; reports the connect endpoint, or an error.
   */
  async function submit() {
    setError(null);
    setBusy(true);
    try {
      const res = await invoke<ManualPairResult>("manual_adb_pair", {
        pairEndpoint: pairAddr,
        code: pairCode,
        connectEndpoint: connectAddr.trim(),
      });
      if (res.connect_endpoint) await onPaired(res.connect_endpoint);
      else setError("Paired, but adb could not connect to that Connect IP:Port.");
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="w-[440px] max-w-[96vw] gap-4 text-sm" onOpenAutoFocus={(e) => e.preventDefault()}>
        <DialogTitle className="flex items-center gap-2 text-lg font-medium text-foreground">
          <KeyRound className="size-5 text-primary" />
          Manual setup
        </DialogTitle>
        <DialogDescription className="text-xs leading-relaxed text-muted-foreground">
          On the device open <b className="text-foreground">Wireless debugging</b>, choose{" "}
          <b className="text-foreground">Pair device with pairing code</b> and enter the address and code it shows. The{" "}
          <b className="text-foreground">Connect IP:Port</b> is different: it is on the main Wireless debugging screen,
          under &lsquo;IP address &amp; Port&rsquo;.
        </DialogDescription>
        <div className="space-y-2">
          <div className="grid grid-cols-[1fr_120px] gap-2">
            <TextInput
              aria-label="Pairing IP:Port"
              className="w-full font-mono"
              placeholder="Pairing IP:Port"
              value={pairAddr}
              onChange={(v) => setPairAddr(v)}
            />
            <TextInput
              aria-label="6-digit pairing code"
              className="w-full font-mono"
              placeholder="6-digit code"
              inputMode="numeric"
              maxLength={6}
              value={pairCode}
              onChange={(v) => setPairCode(v.replace(/\D/g, ""))}
            />
          </div>
          <TextInput
            aria-label="Connect IP:Port (required)"
            className="w-full font-mono"
            placeholder="Connect IP:Port (required)"
            value={connectAddr}
            onChange={(v) => setConnectAddr(v)}
          />
        </div>
        {error && (
          <div role="alert" className="rounded-lg bg-destructive/15 px-3 py-2 text-xs leading-relaxed text-foreground">
            {error}
          </div>
        )}
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button size="sm" onClick={submit} disabled={busy || !pairAddr.trim() || pairCode.length !== 6 || !connectAddr.trim()}>
            {busy ? <LoaderCircle className="animate-spin" /> : <Wifi />}
            Pair and connect
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
