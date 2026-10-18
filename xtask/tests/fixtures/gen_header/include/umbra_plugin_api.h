#ifndef GEN_HEADER_FIXTURE_H
#define GEN_HEADER_FIXTURE_H

#include <stdint.h>

/**
 * A headed struct.
 */
typedef struct {
  /**
   * ABI version.
   */
  uint32_t abi_version;
  /**
   * Struct size.
   */
  uint32_t struct_size;
} FixtureStruct;

#endif  /* GEN_HEADER_FIXTURE_H */
