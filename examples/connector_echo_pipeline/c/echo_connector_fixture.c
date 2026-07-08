#include "num_connectors.h"

#include <string.h>

NumCStatus num_connector_echo_reply(
    NumCString message,
    const NumCContext *context,
    NumCString *out) {
  if (context == NULL) {
    return num_c_error("invalid_context", "NumCContext is required", false);
  }
  if (context->timeout_ms == 0) {
    return num_c_error("missing_timeout", "native calls require timeout_ms", false);
  }
  if (out == NULL) {
    return num_c_error("invalid_out", "result out pointer is required", false);
  }

  static const char prefix[] = "c echo: ";
  (void)message;
  out->data = prefix;
  out->len = strlen(prefix);
  return num_c_ok();
}
