import http from "k6/http";
import { check, sleep } from "k6";

const baseUrl = __ENV.RUSTFLOW_STATIC_BASE_URL || "http://127.0.0.1:3000";
const staticPath = __ENV.RUSTFLOW_STATIC_PATH || "/assets/app.css";

export const options = {
  vus: 10,
  duration: "15s",
  thresholds: {
    "http_req_failed": ["rate<0.01"],
    "http_req_duration": ["p(95)<50"],
  },
};

export default function () {
  const getResponse = http.get(`${baseUrl}${staticPath}`, {
    headers: { accept: "*/*" },
  });
  check(getResponse, {
    "static GET is 200": (response) => response.status === 200,
    "static GET has cache control": (response) =>
      response.headers["Cache-Control"] !== undefined,
  });

  const headResponse = http.head(`${baseUrl}${staticPath}`);
  check(headResponse, {
    "static HEAD is 200": (response) => response.status === 200,
  });

  sleep(0.05);
}
