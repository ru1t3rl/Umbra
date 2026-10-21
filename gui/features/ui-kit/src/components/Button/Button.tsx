import { type ButtonHTMLAttributes, type ReactElement, type ReactNode, useEffect } from "react";
import { Icon } from "../Icon/Icon.tsx";
import "./Button.css";

interface ButtonBaseProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  /** filled = the one azure command per view; tonal is the default; outline and ghost for secondary; danger for destructive. */
  variant?: "filled" | "tonal" | "outline" | "ghost" | "danger";
  size?: "md" | "sm";
}

/** A button with visible content; `icon` is an optional leading glyph. */
interface ButtonWithContentProps extends ButtonBaseProps {
  /** Material Symbols name. */
  icon?: string;
  /** Not `undefined` or `null`: `<Button icon="x">{undefined}</Button>` must not compile without a label. */
  children: NonNullable<ReactNode>;
}

/** An icon-only button has no visible text, so it must carry an accessible name. */
interface ButtonIconOnlyBase extends ButtonBaseProps {
  icon: string;
  children?: undefined;
}
type ButtonIconOnlyProps = ButtonIconOnlyBase &
  ({ "aria-label": string } | { "aria-labelledby": string });

export type ButtonProps = ButtonWithContentProps | ButtonIconOnlyProps;

export function Button({
  variant = "tonal",
  size,
  icon,
  children,
  className,
  ...rest
}: ButtonProps): ReactElement {
  const iconOnly = icon !== undefined && icon !== "" && !children;
  const unnamed = iconOnly && !rest["aria-label"] && !rest["aria-labelledby"];
  useEffect(() => {
    if (unnamed && process.env.NODE_ENV !== "production") {
      console.warn(`@umbra/ui-kit Button: icon-only button "${icon}" needs an aria-label.`);
    }
  }, [unnamed, icon]);
  const classes = [
    "vt-btn",
    `vt-btn-${variant}`,
    size === "sm" && "vt-btn-sm",
    iconOnly && "vt-btn-icon",
    className,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button type="button" {...rest} className={classes}>
      {icon ? <Icon name={icon} /> : null}
      {children}
    </button>
  );
}
