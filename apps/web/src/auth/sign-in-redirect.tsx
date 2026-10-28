import { Navigate, useLocation } from "react-router-dom";
import { loginHref } from "./return-to";

export function SignInRedirect() {
  const { pathname, search, hash } = useLocation();
  return <Navigate to={loginHref(`${pathname}${search}${hash}`)} replace />;
}
