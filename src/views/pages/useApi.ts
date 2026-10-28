import { useEffect, useState } from "react";
import { call } from "../../lib/api";
import { useStore } from "../../lib/store";

/** Fetch an API method, re-fetching whenever the library changes or deps change. */
export function useApi<T>(method: string, params: Record<string, unknown> = {}, deps: unknown[] = []) {
  const { version } = useStore();
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const key = JSON.stringify(params);
  useEffect(() => {
    let live = true;
    setLoading(true);
    call<T>(method, params)
      .then((d) => live && (setData(d), setError(null)))
      .catch((e) => live && setError(String(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [method, key, version, ...deps]);
  return { data, error, loading, setData };
}
