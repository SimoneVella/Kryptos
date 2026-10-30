import type { CSSProperties } from "react";

/**
 * A Material Symbols Rounded icon, from the subset bundled in assets/
 * (scripts/gen-material-icons.mjs collects the names used as `name="..."`).
 * `fill` switches to the solid style.
 */
export function Icon({ name, size = 20, fill, className, style }: { name: string; size?: number; fill?: boolean; className?: string; style?: CSSProperties }) {
  return (
    <span className={`ms${fill ? " ms-fill" : ""}${className ? ` ${className}` : ""}`} style={{ fontSize: size, ...style }} aria-hidden="true">
      {name}
    </span>
  );
}
