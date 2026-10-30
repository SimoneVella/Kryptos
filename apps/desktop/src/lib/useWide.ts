import { useEffect, useState } from "react";

/** Wide enough for the list and the selected entry side by side (desktop windows). */
const QUERY = "(min-width: 1000px)";

export function useWide(): boolean {
  const [wide, setWide] = useState(() => window.matchMedia(QUERY).matches);
  useEffect(() => {
    const mq = window.matchMedia(QUERY);
    const on = () => setWide(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return wide;
}
