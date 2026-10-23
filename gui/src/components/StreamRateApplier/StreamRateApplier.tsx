import { useApplySubscription } from "@umbra/engine-stream";
import { useSettings } from "@umbra/settings";

/**
 * Applies the stored stream rate to the connection, once for the whole app (ASM-997): a page does not have to be
 * the dashboard or the settings page for the rate to be in force. Renders nothing.
 */
export function StreamRateApplier(): null {
  useApplySubscription({ max_rate_hz: useSettings().stream.maxRateHz });
  return null;
}
