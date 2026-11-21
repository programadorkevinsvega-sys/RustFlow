import http from "k6/http";
import { check, sleep } from "k6";

const baseUrl = __ENV.RUSTFLOW_ACTIONS_BASE_URL || "http://127.0.0.1:3001";
const actionPath = __ENV.RUSTFLOW_ACTIONS_PATH || "/api/actions/ping";

export const options = {
  vus: 10,
  duration: "15s",
  thresholds: {
    http_req_failed: ["rate<0.01"],
    http_req_duration: ["p(95)<100"],
    checks: ["rate==1"],
  },
};

export default function () {
  const nonce = __VU * 1_000_000 + __ITER + 1;
  const response = http.post(
    `${baseUrl}${actionPath}`,
    JSON.stringify({ nonce }),
    { headers: { "Content-Type": "application/json" } },
  );
  const payload = response.status === 200 ? response.json() : null;

  check(response, {
    "action returns 200": (result) => result.status === 200,
    "action returns the typed JSON result": () =>
      payload !== null && payload.nonce === nonce,
    "action disables shared caching": (result) =>
      result.headers["Cache-Control"] === "no-store",
    "action response is JSON": (result) =>
      result.headers["Content-Type"].startsWith("application/json"),
  });

  sleep(0.05);
}
