import type { AnchorHTMLAttributes, MouseEvent, ReactElement } from "react";
import { useAppNavigate } from "../../hooks/useAppNavigate.ts";
import "./AppLink.css";

export interface AppLinkProps extends Omit<AnchorHTMLAttributes<HTMLAnchorElement>, "href"> {
  to: string;
}

function isPlainClick(e: MouseEvent): boolean {
  return e.button === 0 && !e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey;
}

/** A link that opens elsewhere or downloads is not an in-app navigation. */
function staysInPage(target: string | undefined, download: unknown): boolean {
  return (target === undefined || target === "" || target === "_self") && download === undefined;
}

export function AppLink({
  to,
  onClick,
  target,
  download,
  className,
  children,
  ...rest
}: AppLinkProps): ReactElement {
  const navigate = useAppNavigate();
  return (
    <a
      {...rest}
      className={["vt-applink", className].filter(Boolean).join(" ")}
      target={target}
      download={download}
      href={to}
      onClick={(e) => {
        onClick?.(e);
        if (e.defaultPrevented || !isPlainClick(e) || !staysInPage(target, download)) return;
        e.preventDefault();
        navigate(to);
      }}
    >
      {children}
    </a>
  );
}
