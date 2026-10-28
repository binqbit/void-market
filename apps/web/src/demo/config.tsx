import { createContext, useContext } from "react";
import type { ReactNode } from "react";
import { useRemote } from "../api/use-remote";

const Context = createContext({ enabled: false, loading: true });

export function DemoProvider({ children }: { children: ReactNode }) {
  const configuration = useRemote<{ google: boolean; demo?: boolean }>(
    "/v1/auth/config",
  );
  return (
    <Context.Provider
      value={{
        enabled: configuration.data?.demo === true,
        loading: configuration.loading,
      }}
    >
      {children}
    </Context.Provider>
  );
}

export function useDemoMode() {
  return useContext(Context).enabled;
}

export function useDemoConfig() {
  return useContext(Context);
}
