import http from "k6/http";
import { check, sleep } from "k6";

const baseUrl = __ENV.RUSTFLOW_STATIC_BASE_URL || "http://127.0.0.1:3000";
const staticPath = __ENV.RUSTFLOW_STATIC_PATH || "/assets/app.css";
const rangePath = __ENV.RUSTFLOW_RANGE_PATH || staticPath;

export const options = {
  vus: 10,
  duration: "15s",
  thresholds: {
    "http_req_failed": ["rate<0.01"],
    "http_req_duration": ["p(95)<100"],
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

  if (__ITER === 0) {
    const largeFileResponse = http.get(`${baseUrl}${rangePath}`);
    check(largeFileResponse, {
      "large static GET is complete": (response) =>
        response.status === 200 && response.body.length === 1024 * 1024,
    });
  }

  const rangeResponse = http.get(`${baseUrl}${rangePath}`, {
    headers: { Range: "bytes=0-1023" },
  });
  check(rangeResponse, {
    "static Range returns requested bytes": (response) =>
      response.status === 206 &&
      response.headers["Content-Range"] !== undefined &&
      response.body.length === 1024,
  });

  const etag = getResponse.headers["Etag"];
  if (etag) {
    const notModified = http.get(`${baseUrl}${staticPath}`, {
      headers: { "If-None-Match": etag },
    });
    check(notModified, {
      "static conditional GET is 304": (response) => response.status === 304,
    });
  }

  sleep(0.05);
}
