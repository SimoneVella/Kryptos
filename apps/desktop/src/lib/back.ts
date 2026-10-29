import { useEffect, useRef } from "react";

// Android's back button: MainActivity calls `window.__kryptosBack()` first, and
// only when it returns false does Android leave the app. Each open sheet (or
// non-home tab) registers here, so back closes the topmost layer first.

const stack: { current: () => void }[] = [];

(window as unknown as { __kryptosBack: () => boolean }).__kryptosBack = () => {
  const top = stack[stack.length - 1];
  if (!top) return false;
  top.current();
  return true;
};

/** While `active`, the back button calls `onBack` instead of leaving the screen. */
export function useBackButton(onBack: () => void, active = true) {
  const ref = useRef(onBack);
  ref.current = onBack;
  useEffect(() => {
    if (!active) return;
    stack.push(ref);
    return () => {
      const i = stack.lastIndexOf(ref);
      if (i >= 0) stack.splice(i, 1);
    };
  }, [active]);
}
