// Agent → screen requests that a page handler cannot do by itself because the
// target state lives inside a child component (e.g. the WhatIf price lives in
// WhatIf.tsx). The `select` ui handler publishes; the component subscribes.

import { useEffect, useRef } from "react";

export interface WhatIfRequest {
  /** OCC symbol of the contract the WhatIf belongs to. */
  ref: string;
  /** Stock price at expiry to show, in USD. */
  price: number;
}

type Listener = (req: WhatIfRequest) => void;

const listeners = new Set<Listener>();
let pending: WhatIfRequest | null = null;

/** Ask the WhatIf of `ref` to show `price`. If no WhatIf is mounted yet (the
 *  page is still opening) the request is kept and delivered on mount. */
export function requestWhatIfPrice(req: WhatIfRequest): void {
  pending = req;
  for (const l of listeners) l(req);
}

/** Subscribe the WhatIf of contract `ref`; `onRequest` receives the price. */
export function useWhatIfRequest(ref: string, onRequest: (price: number) => void): void {
  const cb = useRef(onRequest);
  useEffect(() => {
    cb.current = onRequest;
  });
  useEffect(() => {
    const listener: Listener = (req) => {
      if (req.ref === ref) {
        pending = null;
        cb.current(req.price);
      }
    };
    listeners.add(listener);
    if (pending && pending.ref === ref) listener(pending);
    return () => {
      listeners.delete(listener);
    };
  }, [ref]);
}
