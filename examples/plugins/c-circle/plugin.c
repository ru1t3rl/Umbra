/* SPDX-License-Identifier: MIT OR Apache-2.0 */

/*
 * c-circle: the C example plugin of Umbra (M4 WI-14; M4 acceptance 3, REQ-SDK-2, REQ-P-1, ADR 0023
 * item 3). It is written against the generated header `umbra_plugin_api.h` only, in strict C11
 * (`-std=c11 -pedantic -Wall -Wextra -Werror`, MSVC `/std:c11 /W4 /WX`), and needs nothing but the C
 * runtime (no libm).
 *
 * Plugin `example.plugin.c_circle` 0.1.0 offers one behavior, `example.behavior.c_circle`, that drives
 * its entity on a circle:
 *
 *   - params `speed_mps` (number > 0, default 5) and `turn_rate_dps` (number in [-3600, 3600], default
 *     6: one full circle per minute; positive turns clockwise seen from above);
 *   - every tick it emits SET_SPEED(speed_mps) and SET_HEADING(heading), with
 *     heading = turn_rate_dps * tick / rate_hz degrees (wrapped into [0, 360) and converted to
 *     radians), so the heading is a pure function of simulation time: no wall clock, no private RNG
 *     (REQ-K-4), and no state that could drift;
 *   - it is Running forever;
 *   - it has no `halt_running` entry, so the host calls `halt` instead (ADR 0020); `halt` has nothing
 *     to reset and is idempotent.
 *
 * README.md next to this file walks through the code top to bottom.
 */

#include "umbra_plugin_api.h"

#include <float.h>  /* DBL_MAX */
#include <stdlib.h> /* malloc, free, strtod */
#include <string.h> /* memcpy, memcmp */

/* --------------------------------------------------------------------------------------------------
 * 1. Layout checks. The authoritative asserts are generated from the Rust definitions and compiled by
 *    the host tests (`c_header.rs`); these few show the idea: the ABI is 64-bit only and has no
 *    implicit padding, so these numbers are the same on Linux, macOS and Windows.
 * ------------------------------------------------------------------------------------------------ */
_Static_assert(sizeof(UmbraStr) == 16, "UmbraStr is pointer + length");
_Static_assert(sizeof(UmbraPluginDescriptor) == 88, "descriptor layout of ABI 0.0");
_Static_assert(sizeof(UmbraCapability) == 88, "capability layout of ABI 0.0");
_Static_assert(sizeof(UmbraBehaviorVTable) == 80, "behavior vtable layout of ABI 0.0");
_Static_assert(offsetof(UmbraIntent, value) == 40, "UmbraIntent.value offset of ABI 0.0");

/* --------------------------------------------------------------------------------------------------
 * 2. Small helpers.
 * ------------------------------------------------------------------------------------------------ */

/* Default and limits of the params (the schema below says the same to the host and to users). */
#define CIRCLE_DEFAULT_SPEED_MPS 5.0
#define CIRCLE_DEFAULT_TURN_RATE_DPS 6.0
#define CIRCLE_MAX_TURN_RATE_DPS 3600.0
/* Not in strict C11's <math.h>; spelled out (the closest double to pi). */
#define CIRCLE_PI 3.14159265358979323846

/* The major of a packed `abi_version`. */
#define ABI_MAJOR(v) ((uint32_t)(v) >> 16)

/* A struct written by the host is usable when its major is ours and it is at least as large as the
 * struct we were compiled against (ADR 0019: readers check the major and `struct_size`, never the
 * minor, and never touch fields beyond `struct_size`). */
#define HOST_STRUCT_OK(ptr, type) \
    ((ptr) != NULL && ABI_MAJOR((ptr)->abi_version) == UMBRA_ABI_VERSION_MAJOR && \
     (ptr)->struct_size >= (uint32_t)sizeof(type))

/* The host API passed to `umbra_plugin_entry`; valid for the whole process, usable between `init` and
 * `shutdown`. The only global state of this plugin, reset by `init` and `shutdown` because the host may
 * unload and load the library again in one process. */
static const UmbraHostApi *g_host = NULL;

/* Logs `message` through the host (attributed to this plugin). Logging is best effort: a refused
 * message is not an error of the caller. */
static void log_message(UmbraLogLevel level, UmbraStr message)
{
    if (g_host != NULL && g_host->log != NULL) {
        (void)g_host->log(g_host->host_ctx, level, message);
    }
}

/* Reports `message` through the call's error sink (the host copies it during the call) and returns
 * `code`, so error paths read `return fail(err, CODE, UMBRA_STR("..."));`. */
static UmbraCode fail(const UmbraErrorSink *err, UmbraCode code, UmbraStr message)
{
    if (HOST_STRUCT_OK(err, UmbraErrorSink) && err->set != NULL) {
        err->set(err->ctx, message);
    }
    return code;
}

/* --------------------------------------------------------------------------------------------------
 * 3. Params. The host passes the params as compact JSON text (UTF-8, not NUL-terminated, borrowed for
 *    the call). This plugin accepts exactly what its schema allows: `null` or an object whose members
 *    are `speed_mps` and `turn_rate_dps`, each at most once, each a JSON number. A real plugin would
 *    use a JSON library; this one keeps to the C runtime.
 * ------------------------------------------------------------------------------------------------ */

typedef struct {
    const uint8_t *at;
    const uint8_t *end;
} Cursor;

static void skip_space(Cursor *c)
{
    while (c->at < c->end && (*c->at == ' ' || *c->at == '\t' || *c->at == '\n' || *c->at == '\r')) {
        c->at++;
    }
}

/* Consumes `ch` (after white space); 1 if it was there. */
static int eat(Cursor *c, uint8_t ch)
{
    skip_space(c);
    if (c->at < c->end && *c->at == ch) {
        c->at++;
        return 1;
    }
    return 0;
}

/* Consumes the literal `word` (after white space); 1 if it was there. */
static int eat_word(Cursor *c, const char *word, size_t len)
{
    skip_space(c);
    if ((size_t)(c->end - c->at) >= len && memcmp(c->at, word, len) == 0) {
        c->at += len;
        return 1;
    }
    return 0;
}

/* A member name without escapes (our names have none; a name with escapes is unknown anyway). */
static int parse_key(Cursor *c, const uint8_t **key, size_t *len)
{
    const uint8_t *start;
    if (!eat(c, '"')) {
        return 0;
    }
    start = c->at;
    while (c->at < c->end && *c->at != '"') {
        if (*c->at == '\\' || *c->at < 0x20) {
            return 0;
        }
        c->at++;
    }
    if (c->at == c->end) {
        return 0;
    }
    *key = start;
    *len = (size_t)(c->at - start);
    c->at++; /* closing quote */
    return 1;
}

static int is_digit(const Cursor *c)
{
    return c->at < c->end && *c->at >= '0' && *c->at <= '9';
}

/* A JSON number: checks the grammar `-?(0|[1-9][0-9]*)(.[0-9]+)?([eE][+-]?[0-9]+)?` first, then lets
 * `strtod` (correctly rounded in glibc and the Windows UCRT) convert a NUL-terminated copy. The host
 * process keeps the "C" locale (Rust never calls `setlocale`), so `.` is the decimal point. */
static int parse_number(Cursor *c, double *out)
{
    char text[64];
    const uint8_t *start;
    size_t len;
    char *stop = NULL;

    skip_space(c);
    start = c->at;
    if (c->at < c->end && *c->at == '-') {
        c->at++;
    }
    if (!is_digit(c)) {
        return 0;
    }
    if (*c->at == '0') {
        c->at++;
    } else {
        while (is_digit(c)) {
            c->at++;
        }
    }
    if (c->at < c->end && *c->at == '.') {
        c->at++;
        if (!is_digit(c)) {
            return 0;
        }
        while (is_digit(c)) {
            c->at++;
        }
    }
    if (c->at < c->end && (*c->at == 'e' || *c->at == 'E')) {
        c->at++;
        if (c->at < c->end && (*c->at == '+' || *c->at == '-')) {
            c->at++;
        }
        if (!is_digit(c)) {
            return 0;
        }
        while (is_digit(c)) {
            c->at++;
        }
    }
    len = (size_t)(c->at - start);
    if (len >= sizeof text) {
        return 0;
    }
    memcpy(text, start, len);
    text[len] = '\0';
    *out = strtod(text, &stop);
    return stop == text + len;
}

/* The configured behavior: the checked params. Allocated by `configure`, freed by `destroy_behavior`. */
typedef struct {
    double speed_mps;
    double turn_rate_dps;
} Circle;

/* Fills `params` from the JSON text, or reports why it cannot. */
static UmbraCode parse_params(UmbraStr json, Circle *params, const UmbraErrorSink *err)
{
    Cursor c;
    int seen_speed = 0;
    int seen_turn = 0;

    params->speed_mps = CIRCLE_DEFAULT_SPEED_MPS;
    params->turn_rate_dps = CIRCLE_DEFAULT_TURN_RATE_DPS;
    if (json.ptr == NULL || json.len == 0) {
        return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("params: empty text, expected a JSON object"));
    }
    c.at = json.ptr;
    c.end = json.ptr + json.len;

    if (eat_word(&c, "null", 4)) {
        /* No params: the defaults. */
    } else if (!eat(&c, '{')) {
        return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("params must be a JSON object"));
    } else if (!eat(&c, '}')) {
        for (;;) {
            const uint8_t *key = NULL;
            size_t key_len = 0;
            double value = 0.0;

            if (!parse_key(&c, &key, &key_len) || !eat(&c, ':')) {
                return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("params: malformed object member"));
            }
            if (key_len == 9 && memcmp(key, "speed_mps", 9) == 0) {
                if (seen_speed || !parse_number(&c, &value)) {
                    return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("speed_mps must be one number"));
                }
                if (!(value > 0.0 && value <= DBL_MAX)) {
                    return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("speed_mps must be a finite number > 0"));
                }
                params->speed_mps = value;
                seen_speed = 1;
            } else if (key_len == 13 && memcmp(key, "turn_rate_dps", 13) == 0) {
                if (seen_turn || !parse_number(&c, &value)) {
                    return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("turn_rate_dps must be one number"));
                }
                if (!(value >= -CIRCLE_MAX_TURN_RATE_DPS && value <= CIRCLE_MAX_TURN_RATE_DPS)) {
                    return fail(err, UMBRA_ERR_PARAMS,
                                UMBRA_STR("turn_rate_dps must be a number in [-3600, 3600]"));
                }
                params->turn_rate_dps = value;
                seen_turn = 1;
            } else {
                return fail(err, UMBRA_ERR_PARAMS,
                            UMBRA_STR("unknown param; expected speed_mps or turn_rate_dps"));
            }
            if (eat(&c, '}')) {
                break;
            }
            if (!eat(&c, ',')) {
                return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("params: expected `,` or `}`"));
            }
        }
    }
    skip_space(&c);
    if (c.at != c.end) {
        return fail(err, UMBRA_ERR_PARAMS, UMBRA_STR("params: trailing text after the JSON value"));
    }
    return UMBRA_OK;
}

/* --------------------------------------------------------------------------------------------------
 * 4. The circle. A pure function of (params, tick, rate): every entity on this behavior with the same
 *    params has the same heading at the same tick, whatever happened before.
 * ------------------------------------------------------------------------------------------------ */

/* `degrees` wrapped into [0, 360). Each step is its own statement, so no compiler may fuse the
 * multiply and the subtract into one FMA (C11 contracts only within an expression), which would
 * change the last bit on some CPUs. Beyond 2^53 turns a double has no fraction left; then the value
 * is returned as is (still finite, still a valid heading). */
static double wrap_degrees(double degrees)
{
    double turns = degrees / 360.0;
    double whole;

    if (turns >= 0.0 && turns < 1.0) {
        return degrees;
    }
    if (!(turns > -9.0e15 && turns < 9.0e15)) {
        return degrees;
    }
    whole = 360.0 * (double)(int64_t)turns;
    degrees = degrees - whole;
    if (degrees < 0.0) {
        degrees = degrees + 360.0;
    }
    if (degrees >= 360.0) {
        degrees = degrees - 360.0;
    }
    return degrees;
}

/* Heading in radians at `tick` of a run at `rate_hz` (> 0): turn_rate_dps * tick / rate_hz degrees. */
static double heading_rad(double turn_rate_dps, uint64_t tick, uint32_t rate_hz)
{
    double degrees = turn_rate_dps * (double)tick / (double)rate_hz;
    double wrapped = wrap_degrees(degrees);
    return wrapped * (CIRCLE_PI / 180.0);
}

/* --------------------------------------------------------------------------------------------------
 * 5. The behavior vtable (ADR 0020). Handles are opaque to the host: `UmbraBehavior` and
 *    `UmbraInstance` are incomplete types in the header, and this file decides what they point to
 *    (a `Circle`). The host destroys every instance before the behavior it came from, and never calls
 *    two functions of one plugin at the same time.
 * ------------------------------------------------------------------------------------------------ */

static UmbraCode circle_configure(UmbraStr params, UmbraBehavior **out, const UmbraErrorSink *err)
{
    Circle parsed;
    Circle *behavior;
    UmbraCode code;

    if (out == NULL) {
        return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("configure: null out pointer"));
    }
    *out = NULL;
    code = parse_params(params, &parsed, err);
    if (code != UMBRA_OK) {
        return code;
    }
    behavior = (Circle *)malloc(sizeof *behavior);
    if (behavior == NULL) {
        return fail(err, UMBRA_ERR_INTERNAL, UMBRA_STR("configure: out of memory"));
    }
    *behavior = parsed;
    *out = (UmbraBehavior *)behavior;
    return UMBRA_OK;
}

static void circle_destroy_behavior(UmbraBehavior *behavior)
{
    free(behavior);
}

/* An instance copies the configured params, so it never depends on the behavior's lifetime. */
static UmbraCode circle_instantiate(UmbraBehavior *behavior, uint32_t rate_hz, UmbraInstance **out,
                                    const UmbraErrorSink *err)
{
    Circle *instance;

    if (behavior == NULL || out == NULL) {
        return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("instantiate: null pointer"));
    }
    *out = NULL;
    if (rate_hz == 0) {
        return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("instantiate: rate_hz is 0"));
    }
    instance = (Circle *)malloc(sizeof *instance);
    if (instance == NULL) {
        return fail(err, UMBRA_ERR_INTERNAL, UMBRA_STR("instantiate: out of memory"));
    }
    *instance = *(const Circle *)behavior;
    *out = (UmbraInstance *)instance;
    return UMBRA_OK;
}

static void circle_destroy_instance(UmbraInstance *instance)
{
    free(instance);
}

/* Pushes one intent for batch item `item`; the host copies it during the call. */
static UmbraCode push(const UmbraIntentSink *sink, size_t item, UmbraIntentKind kind, double value)
{
    UmbraIntent intent = {0}; /* unused fields (and `reserved_0`) must be zero */

    intent.abi_version = UMBRA_ABI_VERSION;
    intent.struct_size = (uint32_t)sizeof intent;
    intent.kind = kind;
    intent.value = value;
    return sink->push(sink->ctx, item, &intent);
}

/* Ticks every entity of the batch. The host fills `statuses` with UNSET; we write one status per item.
 * A non-OK result faults this capability and discards the whole batch (ADR 0020). */
static UmbraCode circle_tick_batch(const UmbraBehaviorBatch *batch, const UmbraErrorSink *err)
{
    const UmbraIntentSink *sink;
    size_t i;

    if (!HOST_STRUCT_OK(batch, UmbraBehaviorBatch) || batch->rate_hz == 0 ||
        (batch->count > 0 && (batch->items == NULL || batch->statuses == NULL))) {
        return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("tick_batch: malformed batch"));
    }
    sink = batch->intents;
    if (!HOST_STRUCT_OK(sink, UmbraIntentSink) || sink->push == NULL) {
        return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("tick_batch: malformed intent sink"));
    }
    for (i = 0; i < batch->count; i++) {
        /* The host writes the items in this plugin's layout, so the stride is our sizeof. */
        const UmbraBehaviorItem *item = &batch->items[i];
        const Circle *circle;
        UmbraCode code;

        if (!HOST_STRUCT_OK(item, UmbraBehaviorItem) || item->instance == NULL) {
            return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("tick_batch: malformed item"));
        }
        circle = (const Circle *)item->instance;
        code = push(sink, i, UMBRA_INTENT_SET_SPEED, circle->speed_mps);
        if (code != UMBRA_OK) {
            return code; /* the host refused the intent and has recorded why */
        }
        code = push(sink, i, UMBRA_INTENT_SET_HEADING,
                    heading_rad(circle->turn_rate_dps, batch->tick, batch->rate_hz));
        if (code != UMBRA_OK) {
            return code;
        }
        batch->statuses[i] = UMBRA_STATUS_RUNNING;
    }
    return UMBRA_OK;
}

/* Instance halt: a full reset. This instance keeps no state between ticks, so there is nothing to
 * reset; halting twice, or before the first tick, is fine (idempotent, ASM-096 c). */
static UmbraCode circle_halt(UmbraInstance *instance)
{
    return instance != NULL ? UMBRA_OK : UMBRA_ERR_INVALID_ARGUMENT;
}

/* The vtable: a static with the descriptor's lifetime. Designated initializers leave every entry we
 * do not name null: `halt_running` (the host falls back to `halt`), `set_input` and `output` (the
 * metadata declares no `param_types` and no `outputs`, ADR 0020's "iff" rule). */
static const UmbraBehaviorVTable circle_vtable = {
    .abi_version = UMBRA_ABI_VERSION,
    .struct_size = (uint32_t)sizeof(UmbraBehaviorVTable),
    .configure = circle_configure,
    .destroy_behavior = circle_destroy_behavior,
    .instantiate = circle_instantiate,
    .destroy_instance = circle_destroy_instance,
    .tick_batch = circle_tick_batch,
    .halt = circle_halt,
};

/* --------------------------------------------------------------------------------------------------
 * 6. The descriptor (REQ-P-3): plugin identity, one capability, and the lifecycle hooks.
 * ------------------------------------------------------------------------------------------------ */

/* JSON Schema of the params (the ADR 0009 subset); its top-level `description` is the catalog text. */
#define CIRCLE_SCHEMA                                                                                 \
    "{\"title\":\"example.behavior.c_circle parameters\","                                           \
    "\"description\":\"Drives the entity on a circle: speed `speed_mps`, heading turning at "        \
    "`turn_rate_dps` degrees per second of simulation time (heading = turn_rate_dps * t). "          \
    "Running forever. Written in C against umbra_plugin_api.h.\","                                  \
    "\"type\":\"object\","                                                                           \
    "\"properties\":{"                                                                               \
    "\"speed_mps\":{\"type\":\"number\",\"exclusiveMinimum\":0,\"default\":5.0,"                     \
    "\"description\":\"Speed in meters per second (the motion model clamps it to its limits).\"},"   \
    "\"turn_rate_dps\":{\"type\":\"number\",\"minimum\":-3600,\"maximum\":3600,\"default\":6.0,"     \
    "\"description\":\"Heading change in degrees per second; positive turns clockwise.\"}},"        \
    "\"additionalProperties\":false,"                                                                \
    "\"examples\":[{\"speed_mps\":5.0,\"turn_rate_dps\":6.0}]}"

/* Behavior metadata (ADR 0020): no bindable params, no outputs, only the result text. */
#define CIRCLE_METADATA "{\"result\":\"Running forever\"}"

static const UmbraCapability circle_capabilities[1] = {{
    .abi_version = UMBRA_ABI_VERSION,
    .struct_size = (uint32_t)sizeof(UmbraCapability),
    .kind = UMBRA_CAPABILITY_BEHAVIOR,
    .id = UMBRA_STR_INIT("example.behavior.c_circle"),
    .version = UMBRA_STR_INIT(""), /* empty: the plugin version */
    .param_schema = UMBRA_STR_INIT(CIRCLE_SCHEMA),
    .metadata = UMBRA_STR_INIT(CIRCLE_METADATA),
    .vtable = &circle_vtable,
}};

/* Called once per load, after the host validated the descriptor and before any other call. */
static UmbraCode circle_init(const UmbraHostApi *host, const UmbraErrorSink *err)
{
    if (!HOST_STRUCT_OK(host, UmbraHostApi)) {
        g_host = NULL;
        return fail(err, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_STR("init: unusable host API"));
    }
    g_host = host;
    log_message(UMBRA_LOG_DEBUG, UMBRA_STR("c_circle ready"));
    return UMBRA_OK;
}

/* Called once at unload, after every handle was destroyed. */
static void circle_shutdown(void)
{
    g_host = NULL;
}

static const UmbraPluginDescriptor descriptor = {
    .abi_version = UMBRA_ABI_VERSION, /* the ABI this plugin requires */
    .struct_size = (uint32_t)sizeof(UmbraPluginDescriptor),
    .id = UMBRA_STR_INIT("example.plugin.c_circle"),
    .name = UMBRA_STR_INIT("C circle example"),
    .version = UMBRA_STR_INIT("0.1.0"),
    .capabilities = circle_capabilities,
    .capability_count = sizeof circle_capabilities / sizeof circle_capabilities[0],
    .init = circle_init,
    .shutdown = circle_shutdown,
};

/* --------------------------------------------------------------------------------------------------
 * 7. The one exported symbol (REQ-P-1). Everything above is `static`; UMBRA_EXPORT makes this the
 *    only symbol the library exports. It may be called more than once and returns the same pointer.
 *    The host API is stored by `init`, not here: callbacks made before `init` are refused anyway.
 * ------------------------------------------------------------------------------------------------ */
UMBRA_EXPORT const UmbraPluginDescriptor *umbra_plugin_entry(const UmbraHostApi *host)
{
    (void)host;
    return &descriptor;
}
