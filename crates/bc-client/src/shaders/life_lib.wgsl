// The city's life (`life.rs`): what a car's or a person's `MeshTag` and vertex slots mean, which
// paint each part takes, the cars' lamps, and the dither that fades one in or out. The numbers are
// `bc_client_core::life`'s and `life_mesh::slot`'s (its tests read this file). No imports, so it can
// be checked on its own.

#define_import_path bc::life

// The tags' fields, by their lowest bit. A car: paint and second paint (5 bits each), lamps,
// brake, indicator (2 bits: 1 left, 2 right), parked, sign lit, wear (3 bits). A figure: top and
// bottom (5 bits each, from PEOPLE), skin, hair and shoes (2 bits each). Both: fade (4 bits, 15
// whole) and seed (8 bits).
const CAR_PAINT: u32 = 0u;
const CAR_PAINT2: u32 = 5u;
const CAR_LAMPS: u32 = 10u;
const CAR_BRAKE: u32 = 11u;
const CAR_BLINK: u32 = 12u;
const CAR_PARKED: u32 = 14u;
const CAR_SIGN: u32 = 15u;
const CAR_WEAR: u32 = 16u;
const FIG_TOP: u32 = 0u;
const FIG_BOTTOM: u32 = 5u;
const FIG_SKIN: u32 = 10u;
const FIG_HAIR: u32 = 12u;
const FIG_SHOES: u32 = 14u;
const TAG_FADE: u32 = 19u;
const TAG_SEED: u32 = 23u;

// The palette's entries: where the people's start, their shoes, skin and hair; the cars' trim,
// glass and tyres.
const PEOPLE: u32 = 32u;
const SHOES: u32 = 52u;
const SKIN: u32 = 56u;
const HAIR: u32 = 60u;
const TRIM: u32 = 25u;
const GLASS: u32 = 26u;
const TYRE: u32 = 27u;

// What a vertex is (its colour's red, times 255).
const SLOT_PAINT: u32 = 0u;
const SLOT_PAINT2: u32 = 1u;
const SLOT_TRIM: u32 = 2u;
const SLOT_GLASS: u32 = 3u;
const SLOT_TYRE: u32 = 4u;
const SLOT_HEAD: u32 = 5u;
const SLOT_TAIL: u32 = 6u;
const SLOT_BLINK_L: u32 = 7u;
const SLOT_BLINK_R: u32 = 8u;
const SLOT_DRIVER: u32 = 9u;
const SLOT_SIGN: u32 = 10u;
const SLOT_TOP: u32 = 16u;
const SLOT_BOTTOM: u32 = 17u;
const SLOT_SKIN: u32 = 18u;
const SLOT_HAIR: u32 = 19u;
const SLOT_SHOES: u32 = 20u;

// The lamps, nits (exposed by the hour, as the city's lanterns are): dipped headlamps, running
// lights by day, tail lamps, brake lamps, indicators (1.5 flashes a second), a taxi's sign.
const HEAD_NITS: f32 = 12000.0;
const DRL_NITS: f32 = 3000.0;
const TAIL_NITS: f32 = 1500.0;
const BRAKE_NITS: f32 = 6000.0;
const BLINK_NITS: f32 = 5000.0;
const SIGN_NITS: f32 = 2500.0;
const HEAD_WHITE: vec3<f32> = vec3<f32>(1.0, 0.96, 0.9);
const TAIL_RED: vec3<f32> = vec3<f32>(1.0, 0.04, 0.02);
const AMBER: vec3<f32> = vec3<f32>(1.0, 0.45, 0.05);

// The street's albedo for its bounce (`bc::colony_sky`'s; its tests hold the two the same).
const GROUND_ALBEDO: f32 = 0.2;

// A 4×4 ordered dither's thresholds, 0..15.
const BAYER: array<u32, 16> = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);

fn field(tag: u32, at: u32, bits: u32) -> u32 {
    return (tag >> at) & ((1u << bits) - 1u);
}

// Whether the pixel at `frag` (px) shows something faded to `fade` sixteenths (15 all of it, 0
// none of it).
fn shown(frag: vec2<f32>, fade: u32) -> bool {
    let p = vec2<u32>(frag);
    return BAYER[(p.x & 3u) + 4u * (p.y & 3u)] * 15u < fade * 16u;
}

// Which palette entry a part paints with.
fn paint_index(slot: u32, tag: u32) -> u32 {
    if (slot == SLOT_PAINT) {
        return field(tag, CAR_PAINT, 5u);
    }
    if (slot == SLOT_PAINT2) {
        return field(tag, CAR_PAINT2, 5u);
    }
    if (slot == SLOT_GLASS || slot == SLOT_DRIVER) {
        return GLASS;
    }
    if (slot == SLOT_TYRE) {
        return TYRE;
    }
    if (slot < SLOT_TOP) {
        return TRIM;
    }
    if (slot == SLOT_TOP) {
        return PEOPLE + field(tag, FIG_TOP, 5u);
    }
    if (slot == SLOT_BOTTOM) {
        return PEOPLE + field(tag, FIG_BOTTOM, 5u);
    }
    if (slot == SLOT_SKIN) {
        return SKIN + field(tag, FIG_SKIN, 2u);
    }
    if (slot == SLOT_HAIR) {
        return HAIR + field(tag, FIG_HAIR, 2u);
    }
    return SHOES + field(tag, FIG_SHOES, 2u);
}

// Whether a slot is a lamp.
fn is_lamp(slot: u32) -> bool {
    return (slot >= SLOT_HEAD && slot <= SLOT_BLINK_R) || slot == SLOT_SIGN;
}

// A lamp's glass, unlit.
fn lamp_albedo(slot: u32) -> vec3<f32> {
    if (slot == SLOT_HEAD) {
        return vec3<f32>(0.75, 0.75, 0.72);
    }
    if (slot == SLOT_TAIL) {
        return vec3<f32>(0.35, 0.02, 0.02);
    }
    if (slot == SLOT_SIGN) {
        return vec3<f32>(0.85, 0.8, 0.6);
    }
    return vec3<f32>(0.55, 0.3, 0.04);
}

// A lamp's light, nits: the headlamps dipped when the lamps are on and running lights by day, the
// tail lamps with the city's lamps, the brakes, the indicator while its beat has it lit (the tag
// says, from `bc_client_core::life::lights`), the sign; nothing on a parked car.
fn car_glow(slot: u32, tag: u32) -> vec3<f32> {
    if (field(tag, CAR_PARKED, 1u) == 1u) {
        return vec3<f32>(0.0);
    }
    let lamps = f32(field(tag, CAR_LAMPS, 1u));
    let blink = field(tag, CAR_BLINK, 2u);
    if (slot == SLOT_HEAD) {
        return HEAD_WHITE * mix(DRL_NITS, HEAD_NITS, lamps);
    }
    if (slot == SLOT_TAIL) {
        return TAIL_RED * (TAIL_NITS * lamps + BRAKE_NITS * f32(field(tag, CAR_BRAKE, 1u)));
    }
    if (slot == SLOT_BLINK_L) {
        return AMBER * BLINK_NITS * select(0.0, 1.0, blink == 1u);
    }
    if (slot == SLOT_BLINK_R) {
        return AMBER * BLINK_NITS * select(0.0, 1.0, blink == 2u);
    }
    if (slot == SLOT_SIGN) {
        return AMBER * SIGN_NITS * f32(field(tag, CAR_SIGN, 1u));
    }
    return vec3<f32>(0.0);
}

// The light the street throws back up (nits, to multiply by an albedo and occlusion), as the city's
// walls have it (`bc::colony_sky::bounce_light`): the street lit by the key (colour times lux,
// towards `to_key`) and the sky (`sky`, nits), on whatever faces sideways or down. Bevy's ambient is
// even all round, so without it a car's or a person's side in shade goes black where the walls by
// it stay lit. `n` and `up` (the floor's up there): unit vectors.
fn bounce(n: vec3<f32>, up: vec3<f32>, to_key: vec3<f32>, key: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    let ground = GROUND_ALBEDO * (key * (max(dot(up, to_key), 0.0) / 3.14159265) + sky);
    return ground * (0.5 - 0.5 * dot(n, up));
}
