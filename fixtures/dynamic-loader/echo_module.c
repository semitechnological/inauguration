#include "in_abi.h"

static InCallStatus echo_init(const struct InHostVTable *host) {
  (void)host;
  InCallStatus status = {IN_ABI_OK, 0, 0, 0};
  return status;
}

static InCallStatus echo_shutdown(void) {
  InCallStatus status = {IN_ABI_OK, 0, 0, 0};
  return status;
}

static int32_t echo_add(int32_t a, int32_t b) {
  return a + b;
}

static const void *echo_symbol(const char *name, uint64_t name_len) {
  static const char add_name[] = "echo_add";
  if (name_len == sizeof(add_name) - 1) {
    for (uint64_t i = 0; i < name_len; ++i) {
      if (name[i] != add_name[i]) {
        return 0;
      }
    }
    return (const void *)&echo_add;
  }
  return 0;
}

static const struct InAbiManifest *echo_manifest(void) {
  return 0;
}

static InModuleVTable echo_vtable = {
  IN_ABI_VERSION,
  64,
  0,
  0,
  0,
  0,
  echo_init,
  echo_shutdown,
  echo_symbol,
  echo_manifest,
};

const InModuleVTable *in_module_vtable(void) {
  return &echo_vtable;
}