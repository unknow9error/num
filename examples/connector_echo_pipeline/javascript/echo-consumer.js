// @ts-check

/**
 * JavaScript-side connector implementation sketch.
 *
 * @typedef {import("../generated/connectors").NumConnectors} NumConnectors
 */

/** @type {NumConnectors} */
const connectors = {
  echo: {
    async reply(message, context) {
      const requestId = context?.request_id ?? "no-request";
      return `javascript echo [${requestId}]: ${message}`;
    },
  },
};

module.exports = { connectors };
