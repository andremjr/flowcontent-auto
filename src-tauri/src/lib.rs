use base64::Engine;
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::{
    env,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use url::Url;
use uuid::Uuid;

const REGISTRY_VERSION: u8 = 1;
const WORKSPACE_CONFIG_VERSION: u8 = 1;
const GENERATION_LEDGER_VERSION: u8 = 1;

#[derive(Default)]
struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct UpdaterConfigFile {
    enabled: bool,
    endpoints: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UpdateStatus {
    enabled: bool,
    configured: bool,
    current_version: String,
    endpoints: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UpdateMetadata {
    version: String,
    current_version: String,
}

// ── License result ────────────────────────────────────────────────────────────
#[derive(Serialize, Deserialize, Clone)]
pub struct LicenseResult {
    pub valid: bool,
    pub message: String,
}

// ── Obfuscation Layer ─────────────────────────────────────────────────────────
// XOR key is split across multiple constants to prevent easy extraction.
// The actual key is reconstructed at runtime by interleaving these fragments.

const K_A: [u8; 8] = [0xC7, 0x9F, 0xE2, 0xB6, 0x73, 0x45, 0x2E, 0x66];
const K_B: [u8; 8] = [0x3A, 0x51, 0x84, 0x0D, 0xF8, 0xA1, 0xD9, 0x1B];
const K_C: [u8; 8] = [0x8C, 0xE3, 0x20, 0x5D, 0x08, 0xC4, 0xAB, 0x15];
const K_D: [u8; 8] = [0x4F, 0x97, 0xBA, 0xF1, 0x7C, 0x36, 0x69, 0xDE];

#[inline(never)]
fn reconstruct_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    for i in 0..8 {
        k[i * 2] = K_A[i];
        k[i * 2 + 1] = K_B[i];
    }
    for i in 0..8 {
        k[16 + i * 2] = K_C[i];
        k[16 + i * 2 + 1] = K_D[i];
    }
    k
}

// ── Encoded credential database ───────────────────────────────────────────────
// Each entry is SHA256(token) XOR key — cannot be reversed to original tokens,
// and cannot be pattern-matched as SHA256 hashes in the binary.

static ENCODED_DB: [[u8; 32]; 100] = [
    [
        0xEF, 0xCD, 0x53, 0x6A, 0xA4, 0x3B, 0x08, 0x3A, 0xA4, 0xC6, 0x94, 0x4E, 0x37, 0xDA, 0xB0,
        0xE9, 0x68, 0x8E, 0x2F, 0xF6, 0x03, 0xDD, 0xC9, 0xEF, 0x4C, 0x8E, 0x30, 0x30, 0x74, 0xCB,
        0xCB, 0xFC,
    ],
    [
        0x91, 0x7D, 0x60, 0x90, 0x10, 0x94, 0x47, 0xA5, 0x86, 0xBC, 0xFB, 0x8F, 0xF9, 0x58, 0x00,
        0x8A, 0xFA, 0x2E, 0xC3, 0x88, 0xC2, 0x46, 0xD0, 0xCC, 0x90, 0x98, 0xE1, 0x0B, 0xD3, 0xDD,
        0x65, 0x72,
    ],
    [
        0x53, 0x4A, 0x1F, 0xF7, 0xD3, 0xDF, 0xF2, 0x8A, 0xDA, 0xDB, 0x76, 0xA2, 0x76, 0x49, 0xAA,
        0xF1, 0x08, 0x2B, 0xB5, 0xA9, 0x44, 0xD7, 0x3E, 0x57, 0x93, 0xA2, 0x91, 0x82, 0xC0, 0xCC,
        0x12, 0xFD,
    ],
    [
        0xF7, 0x26, 0x67, 0x05, 0xD8, 0x8A, 0xD1, 0x22, 0x18, 0x38, 0xBB, 0x5D, 0x05, 0xE3, 0xD4,
        0xC8, 0x80, 0xF4, 0x99, 0x8E, 0xF9, 0x37, 0xAE, 0x1F, 0x65, 0x2A, 0xDD, 0x03, 0x16, 0x41,
        0x0B, 0x16,
    ],
    [
        0x46, 0x96, 0xEF, 0x4E, 0xBD, 0xC2, 0x8C, 0xFE, 0xE5, 0x1C, 0x9A, 0xA0, 0xF8, 0x93, 0x83,
        0x3B, 0x14, 0xD2, 0xEA, 0xE1, 0xA5, 0xAE, 0x10, 0x13, 0xB4, 0xA7, 0x46, 0x0B, 0xB1, 0x12,
        0x89, 0xE3,
    ],
    [
        0x7C, 0x85, 0x32, 0x9A, 0x7C, 0xA6, 0x59, 0x56, 0xBC, 0x5B, 0xBE, 0x16, 0x2D, 0xA7, 0xF9,
        0x6C, 0x70, 0x97, 0x73, 0xA9, 0x43, 0x07, 0xED, 0xCA, 0xB9, 0xDE, 0x02, 0x6A, 0x6C, 0x2F,
        0xEB, 0x85,
    ],
    [
        0xB8, 0x19, 0xE4, 0x26, 0x21, 0x7F, 0x12, 0x5F, 0x97, 0xB4, 0x01, 0x8E, 0xB6, 0xBE, 0xE6,
        0x55, 0xB4, 0x4B, 0x5A, 0x5E, 0xCF, 0x11, 0xAB, 0xCF, 0x47, 0x22, 0x7C, 0xD3, 0x7B, 0xFA,
        0xAE, 0xA8,
    ],
    [
        0xCD, 0x2E, 0xEA, 0x3C, 0xA2, 0x2B, 0xC3, 0x08, 0xEC, 0xB8, 0xF2, 0xD3, 0xC2, 0xC1, 0x28,
        0x99, 0xED, 0x7B, 0x00, 0x9C, 0xCA, 0x88, 0x8B, 0xA2, 0xE0, 0xEA, 0xCF, 0xF9, 0xBD, 0x2B,
        0x60, 0x22,
    ],
    [
        0xE8, 0x4B, 0x0B, 0xA5, 0xF2, 0x85, 0x4C, 0x54, 0x3A, 0xC8, 0x0B, 0xFD, 0xA0, 0x07, 0x17,
        0xE7, 0xCC, 0xCC, 0x3E, 0x92, 0x5E, 0xC6, 0xD6, 0x06, 0x64, 0x35, 0xA0, 0xF9, 0xD1, 0x03,
        0x62, 0x9B,
    ],
    [
        0x07, 0xF0, 0xC5, 0xFA, 0xA3, 0x24, 0xAA, 0x86, 0x75, 0x2B, 0xFB, 0xA2, 0x53, 0x83, 0xDA,
        0x66, 0x9D, 0xB0, 0xF5, 0xB3, 0x3A, 0x63, 0x68, 0x39, 0x59, 0xAE, 0x38, 0xD5, 0xA4, 0x54,
        0x27, 0xFA,
    ],
    [
        0x58, 0x3C, 0x4F, 0xA5, 0x8F, 0xE6, 0x90, 0x9C, 0x30, 0x62, 0xBF, 0xAF, 0xDF, 0x07, 0xA0,
        0xDB, 0x11, 0x37, 0x9A, 0xD1, 0x7E, 0xCF, 0x4F, 0x54, 0xB8, 0xD6, 0xB4, 0xF8, 0xBC, 0x16,
        0x19, 0x94,
    ],
    [
        0x0C, 0xEC, 0x1D, 0x01, 0x28, 0x9D, 0x6A, 0xED, 0xD0, 0x73, 0x5F, 0x83, 0x37, 0x81, 0xE4,
        0x25, 0x4E, 0xB5, 0x12, 0xDF, 0xC0, 0x49, 0x98, 0x13, 0x5B, 0x37, 0xC2, 0xF3, 0xA1, 0x91,
        0x4C, 0x7C,
    ],
    [
        0xC5, 0xC8, 0xD6, 0x59, 0x75, 0x23, 0xD0, 0x1B, 0xA1, 0x07, 0x0C, 0xBA, 0x29, 0x0F, 0xF0,
        0x7B, 0x2E, 0x2D, 0xC8, 0x24, 0x03, 0x7F, 0xEC, 0x7D, 0x05, 0x71, 0xBF, 0xFD, 0x46, 0xAD,
        0xAA, 0x75,
    ],
    [
        0x66, 0xA6, 0x26, 0x53, 0x84, 0xC7, 0x37, 0xDE, 0x5B, 0xF9, 0x61, 0xA7, 0x8D, 0x04, 0x6D,
        0x00, 0x1C, 0x13, 0x37, 0x99, 0x4A, 0xF2, 0x4A, 0x26, 0xE2, 0xD7, 0xA2, 0x90, 0x32, 0x06,
        0x1C, 0x13,
    ],
    [
        0x3F, 0xC3, 0xF4, 0xC6, 0xD9, 0xE1, 0x8F, 0xCF, 0x70, 0xCA, 0xF5, 0x36, 0x97, 0xBB, 0xA8,
        0x74, 0xBE, 0x8D, 0x74, 0xCA, 0xFB, 0xF1, 0x94, 0x41, 0xEB, 0xB5, 0x63, 0x46, 0x8B, 0x54,
        0xFE, 0x5B,
    ],
    [
        0xAD, 0xB3, 0xEB, 0xFB, 0x93, 0xEA, 0x3E, 0x5D, 0x07, 0x06, 0x49, 0xAA, 0xCB, 0x4F, 0xF2,
        0x8E, 0x90, 0x71, 0xDD, 0x1F, 0x16, 0x15, 0x45, 0xED, 0xF0, 0x80, 0x60, 0xDF, 0x12, 0xB4,
        0xE8, 0x7D,
    ],
    [
        0x78, 0x5F, 0xDB, 0x9B, 0x3D, 0x97, 0xCF, 0xBF, 0xA9, 0xAA, 0x2A, 0x7D, 0xDA, 0x60, 0x74,
        0x45, 0xF1, 0xFB, 0x32, 0x46, 0xE6, 0x68, 0x7E, 0x26, 0x67, 0xF6, 0xAD, 0xD9, 0xE7, 0x7B,
        0xB1, 0xCB,
    ],
    [
        0xB9, 0xAE, 0x07, 0x3F, 0xED, 0xC5, 0x5F, 0x25, 0x8E, 0xF8, 0xC5, 0x8C, 0xF2, 0x9F, 0x2F,
        0xA5, 0xCE, 0x13, 0xCE, 0x86, 0x70, 0xC0, 0xBE, 0x77, 0x71, 0xB9, 0xA5, 0x4E, 0x9E, 0xF7,
        0x80, 0x5D,
    ],
    [
        0x5F, 0xCE, 0x46, 0x56, 0x71, 0xE8, 0x8D, 0xE7, 0x94, 0x80, 0xCE, 0x40, 0xFF, 0xAB, 0xB3,
        0x26, 0xB1, 0x38, 0x60, 0xE1, 0x58, 0x9D, 0x40, 0xEB, 0xFE, 0x90, 0xA6, 0x55, 0x04, 0xAD,
        0x45, 0xA5,
    ],
    [
        0x3E, 0x48, 0xCB, 0x8E, 0xFD, 0xA8, 0x01, 0xDB, 0xE3, 0xA4, 0x26, 0xBD, 0xAE, 0x75, 0x76,
        0x37, 0x69, 0x5A, 0x42, 0xE9, 0x46, 0xE0, 0xA8, 0x0D, 0xD6, 0x79, 0x6D, 0x4F, 0xE9, 0x87,
        0x11, 0x5C,
    ],
    [
        0x6C, 0xB8, 0xD2, 0xBC, 0xA7, 0x8B, 0x91, 0x29, 0x4C, 0x73, 0x1A, 0x0C, 0xE6, 0x44, 0x65,
        0x4F, 0x4E, 0xF9, 0x56, 0xDF, 0x29, 0x93, 0xA5, 0xE3, 0xA7, 0x76, 0x32, 0x6D, 0x3A, 0x4B,
        0x16, 0x66,
    ],
    [
        0xFE, 0x6D, 0x48, 0x7A, 0xC6, 0x07, 0x02, 0x42, 0xD4, 0xA0, 0x32, 0x9B, 0x46, 0xE0, 0xD8,
        0x6D, 0xB9, 0xE6, 0xFB, 0x70, 0xD1, 0xAC, 0x96, 0xAE, 0x65, 0xB9, 0xD3, 0xBD, 0x22, 0xEA,
        0xC4, 0x10,
    ],
    [
        0x96, 0x44, 0x45, 0xC5, 0x19, 0x8D, 0x6E, 0xF9, 0x12, 0xB5, 0xBA, 0x39, 0x07, 0xBC, 0xAB,
        0xE6, 0x6E, 0x18, 0xFB, 0x77, 0x57, 0x9F, 0x3B, 0xDF, 0x2E, 0xBB, 0x1C, 0xFD, 0x52, 0xED,
        0xD8, 0xBD,
    ],
    [
        0x2F, 0x71, 0xEA, 0x32, 0x20, 0xFA, 0x27, 0xEF, 0xA1, 0x5D, 0xF0, 0xD0, 0x52, 0x87, 0x16,
        0xE8, 0xF9, 0x67, 0x60, 0x0C, 0xCB, 0xD2, 0x31, 0x04, 0x17, 0x18, 0x7E, 0x1A, 0x1E, 0x2D,
        0xD3, 0xFC,
    ],
    [
        0x12, 0x08, 0x6A, 0xB0, 0xEF, 0x13, 0x12, 0xCB, 0x10, 0xDA, 0x31, 0x70, 0x38, 0x57, 0xB8,
        0x58, 0x28, 0xEF, 0xDB, 0xCC, 0x98, 0x5F, 0xF7, 0xE8, 0xE9, 0xC1, 0x73, 0x17, 0xCB, 0xBE,
        0x04, 0x83,
    ],
    [
        0xB1, 0xA4, 0x4D, 0xA3, 0x7C, 0xFD, 0x35, 0x29, 0xD2, 0x14, 0x61, 0xE3, 0xE6, 0x40, 0xE9,
        0xC3, 0x77, 0xAA, 0x0D, 0xC8, 0x7F, 0xBB, 0xA5, 0x05, 0x17, 0x94, 0xE6, 0x1C, 0x01, 0x67,
        0xC7, 0x27,
    ],
    [
        0xB9, 0x76, 0x50, 0x53, 0xB8, 0x10, 0xD6, 0x4C, 0xEA, 0x3C, 0xAA, 0x0E, 0xC9, 0xA0, 0x06,
        0xAE, 0x01, 0xD0, 0xE2, 0x72, 0x9C, 0xDC, 0x61, 0x79, 0xF6, 0xC8, 0x7A, 0x45, 0x12, 0xE4,
        0x85, 0x0C,
    ],
    [
        0x12, 0xE1, 0x18, 0x40, 0x2E, 0x55, 0xC6, 0xBD, 0x73, 0xDC, 0x38, 0x37, 0x72, 0x71, 0x56,
        0x3F, 0x8E, 0x52, 0x0B, 0x5A, 0x30, 0x01, 0x7C, 0xC4, 0x81, 0x74, 0xAB, 0xFF, 0x79, 0xD7,
        0xE9, 0x24,
    ],
    [
        0x12, 0x59, 0x63, 0x53, 0xB5, 0x62, 0xE8, 0x83, 0xDD, 0x23, 0x62, 0xA0, 0x15, 0x8D, 0xAC,
        0x34, 0x4E, 0x27, 0xB2, 0x59, 0xB4, 0x48, 0xE7, 0xC2, 0xBD, 0x60, 0xF8, 0x9B, 0x82, 0xA3,
        0x58, 0xC2,
    ],
    [
        0xC2, 0xAF, 0xF2, 0x7D, 0xEB, 0xD0, 0x19, 0xA1, 0xE3, 0xAA, 0x4A, 0xF0, 0x7B, 0xD1, 0x30,
        0x29, 0x88, 0xA1, 0xDB, 0xDB, 0x29, 0xC3, 0xF0, 0x65, 0xA4, 0x37, 0xE4, 0x03, 0x8A, 0x78,
        0xC8, 0xCB,
    ],
    [
        0xEF, 0xF4, 0x2D, 0x12, 0x91, 0x7E, 0x40, 0x4B, 0x5D, 0xA4, 0x63, 0x50, 0x5D, 0x07, 0x1C,
        0x28, 0x23, 0x2C, 0x61, 0x20, 0x2D, 0x1F, 0xF2, 0x11, 0x51, 0xCD, 0x94, 0x03, 0xF2, 0x62,
        0x52, 0xE2,
    ],
    [
        0x5E, 0x74, 0x9C, 0x07, 0x96, 0xF9, 0x79, 0x24, 0xB9, 0x78, 0x37, 0x90, 0xFE, 0x4A, 0x58,
        0xF8, 0x72, 0x06, 0x35, 0x96, 0x67, 0xB9, 0xE8, 0x64, 0x97, 0x91, 0x57, 0x28, 0xD0, 0x45,
        0xA8, 0x96,
    ],
    [
        0x79, 0x3F, 0xBD, 0xE8, 0x66, 0x5F, 0x6C, 0x29, 0x23, 0x74, 0x04, 0xF7, 0x4A, 0xD9, 0x3D,
        0xC7, 0xB6, 0x50, 0xE0, 0x9A, 0x91, 0xCF, 0x9D, 0x13, 0xBD, 0x7C, 0xE3, 0x17, 0x88, 0x1B,
        0xC9, 0x6E,
    ],
    [
        0x8C, 0xD6, 0xA3, 0x60, 0x1C, 0x26, 0x54, 0x5C, 0xBB, 0x90, 0x28, 0x05, 0xAE, 0xB5, 0x12,
        0xD0, 0xF5, 0x5E, 0x81, 0x0C, 0xAC, 0xCA, 0x62, 0x48, 0x3A, 0x9B, 0xDE, 0x27, 0xA1, 0xD7,
        0x96, 0xC0,
    ],
    [
        0x50, 0x7B, 0x49, 0xB7, 0x81, 0x94, 0x17, 0xE9, 0x30, 0xA9, 0xC9, 0x93, 0x45, 0xE1, 0x99,
        0xFF, 0xE5, 0x14, 0x35, 0x77, 0x35, 0x3F, 0x44, 0xE9, 0x4F, 0x65, 0x58, 0x20, 0x29, 0x77,
        0xDF, 0x63,
    ],
    [
        0xB3, 0x2E, 0xD7, 0x18, 0x10, 0xD6, 0x59, 0x94, 0x84, 0xA0, 0x77, 0xFE, 0xE2, 0xF5, 0xA1,
        0xEA, 0x6B, 0xA4, 0x91, 0x4A, 0x56, 0x2C, 0xE9, 0xA4, 0x6F, 0x0A, 0x11, 0x78, 0xD7, 0xD2,
        0x75, 0x63,
    ],
    [
        0x99, 0xCD, 0x52, 0x94, 0xA8, 0x89, 0xAA, 0x66, 0xD1, 0x2B, 0xE2, 0x0A, 0xE1, 0x31, 0xCB,
        0xBF, 0xC9, 0x11, 0xAD, 0xC1, 0xD9, 0x64, 0xA0, 0x19, 0x5C, 0xF4, 0x1E, 0x0A, 0x50, 0xE6,
        0xF2, 0xD2,
    ],
    [
        0x5A, 0x9B, 0xCC, 0x75, 0xBB, 0x90, 0xDA, 0xCF, 0xA1, 0xE7, 0xCC, 0x90, 0xFF, 0xCC, 0xA4,
        0x56, 0xE7, 0x67, 0xEB, 0xE1, 0xB0, 0x07, 0x96, 0x05, 0x3C, 0x05, 0xE8, 0x8A, 0x4C, 0x36,
        0xC7, 0xAE,
    ],
    [
        0x39, 0x46, 0x37, 0xED, 0xE7, 0x48, 0xA6, 0xD1, 0xD0, 0xE5, 0x55, 0xF9, 0xF1, 0x4A, 0xBD,
        0xC2, 0x97, 0x74, 0xC2, 0xA3, 0x6D, 0xC2, 0x39, 0x80, 0x13, 0xB4, 0x22, 0xF0, 0x26, 0xCD,
        0xB6, 0x00,
    ],
    [
        0x0C, 0xFD, 0x45, 0x6D, 0xC5, 0x61, 0x80, 0xA4, 0xE7, 0x85, 0x3D, 0x85, 0x60, 0xB9, 0x55,
        0xBE, 0x99, 0xF3, 0x54, 0x4F, 0x52, 0xA0, 0x6E, 0x4C, 0xC9, 0x77, 0xBC, 0xD4, 0xCC, 0x4E,
        0xDF, 0x7F,
    ],
    [
        0x80, 0x5E, 0x76, 0x3F, 0xFA, 0x84, 0xFC, 0x87, 0xA3, 0x11, 0xB1, 0xC9, 0x0E, 0xC1, 0x2F,
        0x08, 0xAE, 0xFD, 0x57, 0x13, 0xDE, 0x4E, 0x58, 0xFE, 0x35, 0xF6, 0x61, 0x2C, 0x51, 0xE6,
        0xB2, 0x00,
    ],
    [
        0x10, 0x0C, 0x20, 0xB1, 0x66, 0x9E, 0xD6, 0x8A, 0x70, 0xEF, 0x51, 0x6E, 0x21, 0x0E, 0x76,
        0x3F, 0x78, 0x3F, 0x80, 0x81, 0x6F, 0x5E, 0xFA, 0x11, 0x83, 0x18, 0x9F, 0x85, 0xAC, 0xF0,
        0x4D, 0x38,
    ],
    [
        0xFC, 0xC0, 0x62, 0x88, 0xFD, 0x90, 0xAE, 0x65, 0xC2, 0x9D, 0xD2, 0xE2, 0x1C, 0xDE, 0x60,
        0x4D, 0xA0, 0x9F, 0x18, 0x94, 0xAA, 0xC4, 0x19, 0xC0, 0xB5, 0x19, 0x2C, 0xF8, 0x8B, 0xED,
        0xE8, 0x91,
    ],
    [
        0x2C, 0x7E, 0x01, 0x77, 0x6E, 0x94, 0x05, 0xCB, 0x5D, 0x72, 0xD3, 0x4D, 0x82, 0x35, 0xC1,
        0xB6, 0x57, 0xDE, 0xD1, 0x34, 0x72, 0xA1, 0xF3, 0x9B, 0xFD, 0x03, 0x95, 0x58, 0x1B, 0x00,
        0xFE, 0xD8,
    ],
    [
        0x6D, 0x64, 0xD9, 0x5E, 0xC5, 0x8A, 0x53, 0x6B, 0x6F, 0x8A, 0x95, 0xD3, 0xA3, 0xC8, 0x8E,
        0xC3, 0xBB, 0x5B, 0x2F, 0xFF, 0xBA, 0x83, 0xB0, 0x5F, 0x9B, 0x8F, 0xF1, 0xE3, 0x60, 0x2E,
        0x51, 0x3E,
    ],
    [
        0x51, 0x80, 0xFD, 0xE9, 0x7F, 0xCF, 0x96, 0xFE, 0xE2, 0xD7, 0x85, 0x71, 0xC6, 0x83, 0x99,
        0xF0, 0xAA, 0xDC, 0x55, 0x30, 0xCF, 0x27, 0xE0, 0xC6, 0xCC, 0xDF, 0x7D, 0x28, 0x90, 0xFE,
        0x00, 0x4D,
    ],
    [
        0x00, 0x39, 0x1B, 0xC9, 0x01, 0xC7, 0x40, 0x51, 0xC9, 0x85, 0x84, 0x84, 0x30, 0x1C, 0xE5,
        0xA6, 0x8C, 0x21, 0x7F, 0xE7, 0x8F, 0xFA, 0x75, 0xE1, 0x45, 0xAB, 0x39, 0x1A, 0x45, 0x23,
        0xA3, 0x7E,
    ],
    [
        0x48, 0x1F, 0xBC, 0xC1, 0x3B, 0xA8, 0x72, 0x55, 0x16, 0x0B, 0x31, 0xC8, 0x7D, 0xFD, 0x6D,
        0x0C, 0x64, 0xB3, 0x1D, 0x55, 0x12, 0xB8, 0x99, 0x22, 0x8D, 0x8B, 0x8C, 0x45, 0xD6, 0x8F,
        0x99, 0x60,
    ],
    [
        0x14, 0xB8, 0x97, 0x1E, 0x3C, 0xB5, 0x39, 0x33, 0x59, 0x03, 0x8B, 0xB6, 0xA7, 0x5C, 0x9B,
        0x99, 0x39, 0xD6, 0xCA, 0x94, 0x34, 0xEB, 0x52, 0x3F, 0x0F, 0x95, 0x4D, 0x77, 0x78, 0x6C,
        0xBD, 0xBD,
    ],
    [
        0x94, 0x1B, 0xA9, 0xD2, 0x2F, 0xB4, 0x63, 0x96, 0x01, 0x3E, 0xB3, 0xF0, 0x13, 0x6B, 0x50,
        0xC1, 0x9B, 0x77, 0xAE, 0xF8, 0xBB, 0xCB, 0x82, 0x1F, 0x30, 0xF4, 0x22, 0xB0, 0x7C, 0xC8,
        0xE2, 0xB6,
    ],
    [
        0xF8, 0xF9, 0x1F, 0x96, 0x81, 0x29, 0xD6, 0x6E, 0x4E, 0xF5, 0x7A, 0x08, 0x6D, 0xA9, 0x70,
        0x45, 0xF6, 0xF5, 0xFD, 0x02, 0xD8, 0xF7, 0xE0, 0x44, 0x8D, 0x55, 0xC4, 0x9D, 0xB6, 0x8B,
        0xB3, 0xD9,
    ],
    [
        0x4E, 0x2E, 0xBA, 0xCC, 0xC6, 0xC8, 0x05, 0x65, 0x1D, 0xEB, 0x0D, 0xD3, 0x02, 0x7C, 0xA0,
        0x93, 0xFF, 0x21, 0xB0, 0x01, 0xA8, 0x89, 0xCD, 0xBD, 0x6C, 0x73, 0xCB, 0xDC, 0xAE, 0x2F,
        0x34, 0xF6,
    ],
    [
        0x24, 0xA6, 0x50, 0xF8, 0xB2, 0x9B, 0x70, 0xDF, 0x5C, 0x67, 0x3C, 0x71, 0x28, 0x31, 0x81,
        0x87, 0xE1, 0x90, 0x0D, 0x2D, 0x30, 0xC6, 0x13, 0x14, 0x56, 0x68, 0x2E, 0x51, 0xA7, 0xED,
        0xEF, 0x2C,
    ],
    [
        0x60, 0x0B, 0x07, 0xDE, 0x6B, 0xD8, 0x09, 0x35, 0x97, 0x76, 0x59, 0x30, 0x0D, 0x2A, 0x0B,
        0x35, 0x14, 0xDE, 0x0B, 0xFC, 0x14, 0x3A, 0x84, 0x38, 0x5F, 0x74, 0xDF, 0x3A, 0x5E, 0x17,
        0xE1, 0xC8,
    ],
    [
        0x9A, 0xA0, 0x00, 0x87, 0xBD, 0xBD, 0xD6, 0xDC, 0xB3, 0xD6, 0xEB, 0xBF, 0x08, 0x33, 0x13,
        0xC8, 0xB4, 0x89, 0x79, 0x73, 0xE3, 0x09, 0x1D, 0x08, 0x8A, 0x09, 0x20, 0x47, 0xEE, 0x61,
        0x41, 0x2B,
    ],
    [
        0xAB, 0x23, 0x07, 0x0A, 0x1E, 0x19, 0x17, 0xC2, 0xAC, 0x03, 0x12, 0x1A, 0xBC, 0xEA, 0x0A,
        0x62, 0x61, 0xA6, 0xDD, 0xA4, 0x76, 0xAD, 0x2B, 0xBF, 0x9C, 0x4D, 0xC6, 0xF0, 0x64, 0x09,
        0x06, 0xC4,
    ],
    [
        0x0F, 0x92, 0x86, 0x4D, 0xAE, 0xD7, 0x4D, 0xD3, 0xA7, 0x4A, 0x69, 0xBF, 0xE1, 0x33, 0x80,
        0x17, 0xAA, 0x18, 0x77, 0x36, 0xC4, 0x2A, 0xAA, 0x1E, 0xA1, 0xC7, 0x60, 0x31, 0x4F, 0x9B,
        0xB1, 0x76,
    ],
    [
        0xCB, 0x99, 0xB2, 0x16, 0x6D, 0x7A, 0x9C, 0xA7, 0xEB, 0xBE, 0x04, 0x25, 0x65, 0x98, 0xD9,
        0xF8, 0x1D, 0xE9, 0xA8, 0x8F, 0xF6, 0x3D, 0xD8, 0x0F, 0xCA, 0x08, 0x3E, 0xA5, 0xF5, 0x1E,
        0x56, 0x36,
    ],
    [
        0x70, 0xD1, 0xF0, 0xCD, 0x6B, 0x6A, 0xFC, 0xAD, 0x6C, 0x8B, 0x86, 0x4A, 0x3A, 0xD6, 0x00,
        0x74, 0x96, 0x25, 0x3A, 0xE7, 0x40, 0xC0, 0xD0, 0xCC, 0x5C, 0xE8, 0xA9, 0x2A, 0x37, 0x12,
        0xA4, 0x7A,
    ],
    [
        0x25, 0xBA, 0xAF, 0x58, 0x2B, 0x95, 0xF8, 0x9E, 0x6D, 0x64, 0xFE, 0xD1, 0x32, 0x8A, 0xCA,
        0x39, 0x0A, 0x09, 0x43, 0x1D, 0xAB, 0xCA, 0x35, 0x80, 0xF6, 0xE2, 0xCD, 0xDC, 0x8D, 0xF1,
        0x10, 0x6A,
    ],
    [
        0xF7, 0xAD, 0x30, 0xBE, 0x9C, 0x60, 0x2E, 0xBB, 0xC5, 0xD2, 0xA4, 0x91, 0x35, 0xD1, 0x20,
        0x62, 0x85, 0x4D, 0x13, 0xE3, 0xA7, 0xF8, 0x49, 0x71, 0x7F, 0x2F, 0x68, 0x1D, 0x9C, 0xB0,
        0xDA, 0xF8,
    ],
    [
        0x6E, 0x63, 0x57, 0xBA, 0x06, 0xE2, 0x59, 0x63, 0xE3, 0x49, 0x63, 0xB2, 0x93, 0xF9, 0x5A,
        0x27, 0x06, 0x5D, 0x79, 0xA6, 0xB9, 0x49, 0x44, 0x9D, 0xB4, 0x07, 0x8F, 0xC8, 0xA8, 0xA4,
        0xF0, 0xDC,
    ],
    [
        0x7E, 0xF1, 0x87, 0xF1, 0xFB, 0x16, 0xEF, 0xE7, 0x4D, 0x05, 0xED, 0xBF, 0xE5, 0x62, 0x07,
        0x32, 0x03, 0x27, 0x84, 0x63, 0xC1, 0xAC, 0xAC, 0xA8, 0x27, 0x2A, 0x8E, 0x53, 0x6C, 0xBC,
        0x6D, 0xB8,
    ],
    [
        0x9A, 0x63, 0xA9, 0x12, 0xD9, 0xB8, 0x7D, 0x13, 0x50, 0x86, 0x47, 0x80, 0x98, 0xA4, 0x11,
        0x2C, 0x90, 0x44, 0xD1, 0x49, 0x6E, 0x0A, 0xAC, 0xFF, 0x53, 0x33, 0x8F, 0x89, 0x46, 0x3C,
        0x61, 0x57,
    ],
    [
        0x5E, 0x13, 0x56, 0xF8, 0x79, 0xA3, 0x29, 0x9F, 0x4C, 0x2C, 0xC4, 0x76, 0x6F, 0xD7, 0xBC,
        0xFC, 0x9B, 0x72, 0xED, 0x63, 0x08, 0x17, 0xB6, 0x79, 0xF0, 0x36, 0xA4, 0xC8, 0xF5, 0x35,
        0x2A, 0xEC,
    ],
    [
        0xBF, 0xD7, 0x76, 0x6A, 0x0C, 0x1C, 0xA2, 0x10, 0x2D, 0x6D, 0x06, 0x93, 0x47, 0x23, 0x60,
        0xC8, 0x74, 0x76, 0x60, 0xC0, 0x0C, 0x2A, 0x0C, 0xD1, 0x5D, 0xC2, 0x63, 0x2E, 0x75, 0x91,
        0x73, 0x9B,
    ],
    [
        0x5F, 0x15, 0x40, 0x3A, 0x5A, 0xB2, 0x0F, 0xE6, 0x56, 0x87, 0x5E, 0x10, 0x06, 0xE1, 0x2E,
        0x1F, 0xD3, 0x5E, 0xDA, 0xB5, 0x99, 0x52, 0x39, 0x55, 0xDC, 0x20, 0x97, 0x37, 0xED, 0xC4,
        0x10, 0x8F,
    ],
    [
        0x17, 0x38, 0xBF, 0xB4, 0x58, 0x51, 0xE7, 0x94, 0x92, 0x32, 0x49, 0xBD, 0xA5, 0xAE, 0x6A,
        0xE2, 0xA7, 0x90, 0xD6, 0x41, 0x89, 0xF3, 0xC9, 0x13, 0x31, 0x21, 0x59, 0xB3, 0x5D, 0x15,
        0x2E, 0xA9,
    ],
    [
        0xC1, 0x2C, 0x5D, 0x91, 0xF2, 0x75, 0x63, 0x70, 0x81, 0x33, 0x15, 0xA0, 0xEC, 0xB8, 0x76,
        0x79, 0x24, 0x5D, 0xC2, 0x1A, 0xF2, 0xBE, 0xD1, 0x09, 0xF4, 0x76, 0x93, 0x39, 0x40, 0xCE,
        0xCD, 0xD8,
    ],
    [
        0x28, 0x02, 0x80, 0xAD, 0x17, 0x3D, 0x82, 0xD7, 0xED, 0x29, 0xCF, 0x21, 0x57, 0x46, 0x23,
        0x67, 0xDF, 0x6F, 0x71, 0x5A, 0xE7, 0x59, 0xD4, 0x4C, 0xF7, 0x4D, 0x1B, 0xFD, 0x33, 0xB4,
        0x29, 0x36,
    ],
    [
        0x0F, 0xD4, 0x0D, 0x85, 0xA9, 0x67, 0x86, 0x36, 0x33, 0xAA, 0xA8, 0x31, 0x38, 0x92, 0xFD,
        0x82, 0xDD, 0x5D, 0xF5, 0xC9, 0x86, 0x13, 0x33, 0xEF, 0x2C, 0x80, 0xA0, 0x5C, 0xE5, 0x2B,
        0x4A, 0x6F,
    ],
    [
        0xDA, 0x44, 0x07, 0x51, 0xB1, 0x74, 0x6C, 0x51, 0x83, 0x49, 0x42, 0x30, 0xB7, 0xB6, 0xBE,
        0x3A, 0x87, 0x6E, 0x27, 0xF0, 0x2E, 0x5B, 0x6D, 0x20, 0x0F, 0xC0, 0xC9, 0xE6, 0xAE, 0xE1,
        0xAE, 0x37,
    ],
    [
        0xA3, 0xD6, 0x1C, 0xC5, 0x10, 0xE7, 0x7E, 0x05, 0x86, 0x47, 0xC8, 0xB3, 0x87, 0x55, 0x3A,
        0xD2, 0xA2, 0x69, 0xB9, 0x88, 0x87, 0xAA, 0x3B, 0x5C, 0x4F, 0x4B, 0x7F, 0x8C, 0x77, 0xC7,
        0xDD, 0xE5,
    ],
    [
        0x64, 0xC5, 0x25, 0x7C, 0x6F, 0x19, 0x64, 0x47, 0x4C, 0x7D, 0x6D, 0xF3, 0x19, 0x6E, 0xCD,
        0x2E, 0x90, 0xD7, 0x39, 0xF9, 0xB7, 0x48, 0xB6, 0xFA, 0x63, 0x5D, 0x22, 0x90, 0x4B, 0x16,
        0x0A, 0xCD,
    ],
    [
        0x5A, 0xB0, 0x81, 0x8E, 0x57, 0xD5, 0xC5, 0x19, 0xF4, 0x31, 0x62, 0x6E, 0xAE, 0x7C, 0x8A,
        0x1A, 0xFF, 0xFB, 0xB7, 0x76, 0x4B, 0x90, 0x1D, 0x49, 0x58, 0x16, 0x3E, 0x9E, 0x34, 0xD1,
        0xF7, 0x69,
    ],
    [
        0xCA, 0x43, 0x8A, 0x0E, 0x89, 0xE9, 0x68, 0xC3, 0x4A, 0x74, 0x5A, 0xEF, 0x25, 0x0F, 0xDE,
        0xAC, 0xF3, 0xF3, 0xFE, 0x4A, 0x3E, 0xC4, 0x08, 0x31, 0x12, 0xD6, 0x60, 0x3D, 0xE4, 0x85,
        0x3A, 0x3B,
    ],
    [
        0x07, 0xA9, 0xAD, 0xF0, 0x32, 0x27, 0xF9, 0xAD, 0x42, 0x37, 0xB2, 0x02, 0x78, 0x7A, 0xCF,
        0x0F, 0xBC, 0x9F, 0xA5, 0x2D, 0x6D, 0x42, 0x51, 0x70, 0xC2, 0x26, 0xC7, 0xE8, 0xFE, 0xF2,
        0x18, 0x62,
    ],
    [
        0xAB, 0x3B, 0xE8, 0xA6, 0xC5, 0x4C, 0xC5, 0x80, 0xC3, 0x00, 0xED, 0xD2, 0x9E, 0xBB, 0xF8,
        0x2A, 0x92, 0xD3, 0x61, 0x64, 0xC1, 0x6F, 0x58, 0x01, 0xF5, 0xF6, 0x9C, 0xB0, 0x4E, 0x88,
        0x74, 0x9A,
    ],
    [
        0xCD, 0x61, 0x88, 0xD1, 0xC2, 0xDD, 0x98, 0x39, 0xE7, 0x1C, 0x65, 0x37, 0x59, 0x78, 0x31,
        0x98, 0xDA, 0x7C, 0x56, 0x96, 0xE1, 0xE8, 0xE6, 0x54, 0x96, 0x20, 0x01, 0xB4, 0x42, 0xB4,
        0x69, 0xA1,
    ],
    [
        0x41, 0x11, 0xDD, 0xF4, 0xD3, 0x9B, 0x6D, 0xEC, 0x72, 0x6F, 0x4F, 0x70, 0xA7, 0xC9, 0x7D,
        0x6C, 0x83, 0x54, 0x77, 0x74, 0xF4, 0xC1, 0x21, 0x3C, 0x3E, 0x4F, 0xD2, 0x5C, 0x83, 0x2F,
        0xF6, 0x58,
    ],
    [
        0xB4, 0xF5, 0x41, 0xFF, 0xA8, 0xBB, 0xE9, 0xB8, 0x3F, 0xAC, 0xAD, 0x0A, 0xDC, 0x80, 0xC0,
        0xD4, 0x1F, 0x02, 0x17, 0x3F, 0x37, 0xA7, 0x3E, 0xCD, 0xA8, 0x1F, 0x18, 0x11, 0x5E, 0x6C,
        0x42, 0xC4,
    ],
    [
        0x77, 0xCD, 0xB7, 0xA3, 0xFC, 0x8F, 0x65, 0xFE, 0xA2, 0x77, 0x12, 0x82, 0x7C, 0xBB, 0xCF,
        0x1A, 0x5B, 0xEE, 0xBD, 0xA6, 0xF1, 0x65, 0x1A, 0xC2, 0xB8, 0xC7, 0x9D, 0x03, 0xCD, 0x7B,
        0xA8, 0xF1,
    ],
    [
        0x7B, 0xA8, 0x2F, 0x88, 0x2F, 0x9D, 0x09, 0x75, 0x11, 0xCF, 0xFA, 0xDC, 0xF0, 0xC9, 0x57,
        0x0A, 0xDC, 0xBD, 0xD4, 0x13, 0x1E, 0x3C, 0xAE, 0xC5, 0x20, 0xA8, 0x0A, 0xC7, 0x8A, 0x5B,
        0xE8, 0x0E,
    ],
    [
        0x82, 0xEC, 0xD9, 0x99, 0xD2, 0x55, 0x3B, 0xBE, 0x04, 0xAF, 0x9B, 0x1C, 0x9C, 0x5A, 0x1D,
        0xB2, 0x67, 0xB1, 0x74, 0x34, 0x03, 0x42, 0x32, 0x82, 0x7F, 0xE1, 0x40, 0x6E, 0x5E, 0x9E,
        0x16, 0x24,
    ],
    [
        0x77, 0x00, 0x82, 0x88, 0xBB, 0x69, 0xF2, 0xFA, 0xC2, 0x7D, 0xEA, 0x5C, 0xB5, 0x4A, 0xF2,
        0x64, 0x37, 0xAE, 0xC0, 0x69, 0x6F, 0x69, 0x3E, 0x64, 0x0C, 0x0B, 0xDA, 0x81, 0x6D, 0x03,
        0xDF, 0x43,
    ],
    [
        0xA3, 0x5D, 0x8F, 0x5C, 0xEB, 0x9B, 0xE2, 0xA5, 0x9D, 0x2E, 0x9E, 0x32, 0x2C, 0x70, 0x46,
        0xAB, 0x5E, 0x72, 0x18, 0x7F, 0x8C, 0x90, 0xE7, 0xB0, 0xB1, 0x13, 0x8D, 0xE8, 0x21, 0xCB,
        0x05, 0xC9,
    ],
    [
        0xB4, 0x21, 0xC3, 0x7A, 0x37, 0xF4, 0x5A, 0x75, 0x17, 0x38, 0xF4, 0x18, 0x6B, 0xBD, 0x57,
        0x51, 0x78, 0x4D, 0xDA, 0xDD, 0x56, 0xA6, 0x0E, 0x26, 0x7E, 0x50, 0xC0, 0xD8, 0xC5, 0xCE,
        0x65, 0xAD,
    ],
    [
        0x80, 0x53, 0xBB, 0xF5, 0xCD, 0x38, 0x83, 0x4D, 0x52, 0x85, 0xD8, 0x82, 0x51, 0x9E, 0x2A,
        0x3E, 0x88, 0xE7, 0x68, 0x67, 0xFE, 0xF2, 0x5B, 0xA3, 0x6D, 0x36, 0x79, 0x95, 0xCD, 0x3A,
        0x8C, 0x52,
    ],
    [
        0xCB, 0x59, 0xAE, 0x3A, 0x65, 0xF2, 0xF6, 0x66, 0x14, 0x53, 0xC3, 0xA9, 0x53, 0xEC, 0xBD,
        0x84, 0x4C, 0x4E, 0x90, 0x70, 0x48, 0xFA, 0x01, 0x1A, 0xAD, 0xFB, 0x24, 0x56, 0x78, 0xDD,
        0x58, 0xAA,
    ],
    [
        0xDA, 0x9A, 0xF9, 0xB8, 0xC2, 0x6D, 0xD1, 0xC4, 0xE4, 0x26, 0xD6, 0xB2, 0x3F, 0x76, 0x4C,
        0xC7, 0x84, 0x6A, 0x0E, 0x9E, 0xD8, 0x54, 0xC6, 0xB9, 0xB9, 0xF2, 0x86, 0x46, 0xF0, 0x6B,
        0xB5, 0x94,
    ],
    [
        0x20, 0x02, 0x6F, 0x95, 0xB5, 0xF5, 0xC2, 0xC0, 0xE7, 0xB2, 0x76, 0x21, 0xCA, 0xB8, 0x9F,
        0x3A, 0xA9, 0x36, 0xC6, 0x58, 0xF8, 0xA0, 0x61, 0x55, 0x62, 0x67, 0x38, 0x2F, 0xE1, 0x61,
        0xF0, 0x41,
    ],
    [
        0x21, 0x38, 0xAB, 0x15, 0x43, 0x0A, 0x39, 0x85, 0x00, 0x58, 0x60, 0x32, 0x53, 0x12, 0x24,
        0x53, 0xB1, 0xA6, 0xAE, 0x1B, 0x69, 0xE0, 0x8D, 0x53, 0x85, 0x4D, 0xE3, 0xC0, 0x89, 0xAB,
        0x9E, 0x8C,
    ],
    [
        0xF5, 0x84, 0xBA, 0x9A, 0x08, 0x4B, 0x2C, 0x32, 0x3C, 0x3A, 0x02, 0xAA, 0x85, 0x21, 0x25,
        0x57, 0xF2, 0x16, 0x07, 0x6E, 0xBE, 0x78, 0xFA, 0xEC, 0x59, 0xAE, 0x3B, 0x03, 0x9B, 0xBE,
        0x52, 0x8E,
    ],
    [
        0x74, 0x2C, 0xBE, 0x8C, 0xC4, 0x97, 0x6E, 0xB3, 0xA0, 0xAB, 0x36, 0x06, 0xDD, 0x2D, 0xA9,
        0x64, 0xCE, 0x59, 0x59, 0x79, 0xBE, 0xCD, 0xB0, 0x0D, 0x45, 0x40, 0x6E, 0xAF, 0x2E, 0x9C,
        0xAB, 0xF4,
    ],
    [
        0xDE, 0x33, 0x83, 0xC8, 0x99, 0x20, 0x15, 0xBA, 0xDA, 0xA0, 0x5E, 0x2B, 0x94, 0x99, 0x93,
        0x58, 0xCE, 0xE5, 0x0B, 0x42, 0x27, 0x80, 0x05, 0x5D, 0x78, 0xE4, 0xCA, 0x59, 0x1E, 0xC4,
        0xA5, 0x45,
    ],
    [
        0x77, 0x19, 0xB3, 0x2B, 0x97, 0x75, 0x8E, 0x84, 0x72, 0xFF, 0x7C, 0x07, 0x3D, 0x0A, 0x68,
        0xF4, 0xFB, 0x14, 0xA9, 0x3A, 0x57, 0x00, 0xBC, 0x44, 0x9E, 0xE7, 0x1A, 0x4D, 0x71, 0x4A,
        0x1C, 0xDF,
    ],
    [
        0x2C, 0x22, 0xF2, 0x80, 0x50, 0x07, 0x89, 0xDA, 0x74, 0x40, 0x16, 0xC2, 0xD7, 0x9A, 0x06,
        0x35, 0xF8, 0xB9, 0x24, 0xAB, 0x62, 0x4A, 0xE2, 0x7B, 0x3C, 0xA8, 0x12, 0x4D, 0x81, 0xFC,
        0xBE, 0x8E,
    ],
    [
        0x00, 0xD8, 0xB5, 0x7C, 0x7A, 0xEC, 0x10, 0x22, 0xAC, 0x6C, 0xBF, 0xFD, 0x9B, 0x0C, 0x68,
        0x6C, 0x10, 0xE0, 0xB7, 0x42, 0x2E, 0xCD, 0x2A, 0xBC, 0xB8, 0x74, 0xE9, 0xCE, 0x80, 0x48,
        0x33, 0x97,
    ],
    [
        0x8A, 0x9D, 0xE0, 0xF7, 0x71, 0x60, 0xC6, 0xAE, 0x53, 0xAA, 0x15, 0x17, 0x59, 0x0D, 0xE7,
        0x71, 0xD0, 0x13, 0x9E, 0xB4, 0x36, 0x2C, 0x72, 0x88, 0x89, 0x72, 0x90, 0xA7, 0x1A, 0x5B,
        0x17, 0x3F,
    ],
    [
        0x75, 0x3A, 0x1C, 0x29, 0x02, 0xEF, 0xCE, 0xFC, 0xD9, 0x06, 0xCC, 0x5B, 0xE1, 0xAC, 0xFB,
        0xB9, 0x4B, 0x71, 0xA8, 0xA4, 0x21, 0xE5, 0x78, 0x78, 0x96, 0xFF, 0xF1, 0x0F, 0x52, 0x60,
        0xAF, 0x1F,
    ],
];

// ── Validation core ───────────────────────────────────────────────────────────

#[inline(never)]
fn compute_fingerprint(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

#[inline(never)]
fn check_credential(input: &str) -> bool {
    let normalized = input.trim().to_uppercase();
    let fingerprint = compute_fingerprint(normalized.as_bytes());
    let key = reconstruct_key();

    let mut encoded = [0u8; 32];
    for i in 0..32 {
        encoded[i] = fingerprint[i] ^ key[i];
    }

    // Constant-time comparison against all entries (prevents timing attacks)
    let mut found = false;
    for entry in ENCODED_DB.iter() {
        let mut matches = true;
        for i in 0..32 {
            if encoded[i] != entry[i] {
                matches = false;
            }
        }
        if matches {
            found = true;
        }
    }
    found
}

// ── License file persistence ──────────────────────────────────────────────────

fn license_file() -> PathBuf {
    dirs_next::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("FlowContentAuto")
        .join("license.dat")
}

fn current_month_stamp() -> String {
    chrono::Local::now().format("%Y-%m").to_string()
}

fn save_license(key: &str) -> Result<(), String> {
    let path = license_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Store "key|YYYY-MM" — the month stamp determines expiration
    let payload = format!("{}|{}", key, current_month_stamp());
    let obfuscated: Vec<u8> = payload
        .bytes()
        .enumerate()
        .map(|(i, b)| b ^ (0x42u8.wrapping_add(i as u8).wrapping_mul(0x7B)))
        .collect();
    fs::write(&path, obfuscated).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_license() -> Option<String> {
    let path = license_file();
    let data = fs::read(&path).ok()?;
    let payload: String = data
        .iter()
        .enumerate()
        .map(|(i, &b)| (b ^ (0x42u8.wrapping_add(i as u8).wrapping_mul(0x7B))) as char)
        .collect();
    let payload = payload.trim().to_string();

    // Parse "key|YYYY-MM"
    if let Some(pipe_pos) = payload.rfind('|') {
        let key = &payload[..pipe_pos];
        let saved_month = &payload[pipe_pos + 1..];
        let now_month = current_month_stamp();

        if saved_month == now_month {
            // Same month — license still valid
            Some(key.to_string())
        } else {
            // Month changed — license expired, delete the file
            let _ = fs::remove_file(&path);
            None
        }
    } else {
        // Old format without month — treat as expired
        let _ = fs::remove_file(&path);
        None
    }
}
#[derive(Default)]
struct AuthState {
    authenticated: Mutex<bool>,
}

#[derive(Default)]
struct DiagnosticState {
    writer: Mutex<()>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct FlowBridgeStatus {
    server_ready: bool,
    browser_opened: bool,
    extension_installed: bool,
    extension_connected: bool,
    flow_page_detected: bool,
    flow_url: Option<String>,
    project_id: Option<String>,
    page_title: Option<String>,
    last_heartbeat_ms: Option<u64>,
    chrome_profile: Option<PathBuf>,
    extension_path: Option<PathBuf>,
    pending_command: Option<String>,
    last_command_error: Option<String>,
}

#[derive(Clone)]
struct FlowBridgeState {
    token: String,
    status: Arc<Mutex<FlowBridgeStatus>>,
    pending_commands: Arc<Mutex<HashMap<String, Value>>>,
    acknowledged_commands: Arc<Mutex<HashSet<String>>>,
    command_results: Arc<Mutex<Vec<Value>>>,
    ws_sender: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
    app_handle: Arc<Mutex<Option<tauri::AppHandle>>>,
    generation_queue: Arc<Mutex<Option<GenerationQueueState>>>,
}

#[derive(Clone)]
struct GenerationQueueState {
    active: bool,
    paused: bool,
    local_project_id: String,
    flow_project_id: String,
    project_root: PathBuf,
    mode: String,
    image_model: String,
    video_model: String,
    i2v_model: String,
    image_aspect_ratio: String,
    video_aspect_ratio: String,
    prompts: Vec<Value>, // all prompts [{sourceOrder, prompt}, ...]
    all_prompts: Vec<Value>,
    phase: String,
    next_index: usize, // next prompt to dispatch
    completed_assets: Vec<Value>,
    failed_slots: Vec<(usize, String)>,
    total_prompts: usize,
    max_concurrent: usize,
    target_source_orders: Vec<usize>,
    current_batch_source_orders: Vec<usize>,
    in_flight: HashMap<String, usize>, // command_id -> source_order
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GenerationProgress {
    local_project_id: Option<String>,
    active: bool,
    total_prompts: usize,
    completed_prompts: usize,
    failed_slots: Vec<Value>,
    current_index: usize,
    in_flight: usize,
    paused: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceConfig {
    version: u8,
    workspace_root: PathBuf,
}

const FLOW_MAX_CONCURRENT: usize = 4;
const IMAGE_TO_VIDEO_PHASE_GENERATE: &str = "GENERATE_IMAGES";
const IMAGE_TO_VIDEO_PHASE_ANIMATE: &str = "ANIMATE_IMAGES";

#[derive(Clone)]
struct PythonCommandCandidate {
    program: OsString,
    prefix_args: Vec<OsString>,
}

fn python_command_candidates() -> Vec<PythonCommandCandidate> {
    let mut candidates = Vec::new();
    if let Ok(configured) = env::var("FLOWCONTENT_PYTHON") {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            candidates.push(PythonCommandCandidate {
                program: OsString::from(trimmed),
                prefix_args: Vec::new(),
            });
        }
    }
    candidates.push(PythonCommandCandidate {
        program: OsString::from("python"),
        prefix_args: Vec::new(),
    });
    candidates.push(PythonCommandCandidate {
        program: OsString::from("py"),
        prefix_args: vec![OsString::from("-3")],
    });
    candidates
}

fn build_generation_slots(prompts: &[Value], mode: &str) -> Vec<Value> {
    prompts
        .iter()
        .enumerate()
        .map(|(index, prompt)| {
            let source_order = prompt
                .get("sourceOrder")
                .and_then(Value::as_u64)
                .unwrap_or((index + 1) as u64);
            let prompt_text = prompt.get("prompt").and_then(Value::as_str).unwrap_or("");
            json!({
                "slotId": format!("slot_{source_order:04}"),
                "sourceOrder": source_order,
                "prompt": prompt_text,
                "status": "queued",
                "assetType": match mode {
                    "VIDEO" | "IMAGE_TO_VIDEO" => "video",
                    _ => "image"
                },
                "activeAttemptId": Value::Null,
                "attemptCount": 0,
                "attempts": Vec::<Value>::new(),
                "commandId": Value::Null,
                "workflowId": Value::Null,
                "batchId": Value::Null,
                "operationId": Value::Null,
                "thumbnailUrl": Value::Null,
                "remoteStatus": "LOCAL_QUEUED",
                "remoteUpdatedAt": Value::Null,
                "remainingCredits": Value::Null,
                "currentFileType": Value::Null,
                "localPath": Value::Null,
                "remoteUrl": Value::Null,
                "mediaId": Value::Null,
                "imageMediaId": Value::Null,
                "error": Value::Null
            })
        })
        .collect()
}

fn slot_id_for_source_order(source_order: usize) -> String {
    format!("slot_{source_order:04}")
}

fn new_attempt_id() -> String {
    format!("attempt_{}", Uuid::new_v4())
}

fn attempt_kind_for_command_type(command_type: &str) -> &'static str {
    match command_type {
        "GENERATE_VIDEO" => "TEXT_TO_VIDEO",
        "ANIMATE_IMAGE" => "IMAGE_TO_VIDEO",
        "GENERATE_VIDEO_FROM_IMAGE" => "IMAGE_PLUS_VIDEO",
        _ => "IMAGE",
    }
}

fn ensure_slot_shape(slot: &mut serde_json::Map<String, Value>, source_order: usize) {
    slot.entry("slotId".to_string())
        .or_insert_with(|| json!(slot_id_for_source_order(source_order)));
    slot.entry("sourceOrder".to_string())
        .or_insert_with(|| json!(source_order));
    slot.entry("activeAttemptId".to_string())
        .or_insert(Value::Null);
    slot.entry("attemptCount".to_string())
        .or_insert_with(|| json!(0));
    slot.entry("attempts".to_string())
        .or_insert_with(|| json!([]));
    slot.entry("commandId".to_string()).or_insert(Value::Null);
    slot.entry("workflowId".to_string()).or_insert(Value::Null);
    slot.entry("batchId".to_string()).or_insert(Value::Null);
    slot.entry("operationId".to_string()).or_insert(Value::Null);
    slot.entry("thumbnailUrl".to_string())
        .or_insert(Value::Null);
    slot.entry("remoteStatus".to_string())
        .or_insert_with(|| json!("LOCAL_QUEUED"));
    slot.entry("remoteUpdatedAt".to_string())
        .or_insert(Value::Null);
    slot.entry("remainingCredits".to_string())
        .or_insert(Value::Null);
}

fn update_generation_slot(
    project_root: &Path,
    source_order: usize,
    values: Value,
) -> Result<(), String> {
    if source_order == 0 {
        return Err("Slot de geracao invalido: sourceOrder=0.".to_string());
    }
    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    let production_obj = production
        .as_object_mut()
        .ok_or_else(|| "Arquivo de producao invalido.".to_string())?;
    let slots = production_obj
        .entry("generationSlots".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| "Lista de slots invalida.".to_string())?;

    let slot_index = slots.iter().position(|slot| {
        slot.get("sourceOrder").and_then(Value::as_u64) == Some(source_order as u64)
    });
    let index = if let Some(index) = slot_index {
        index
    } else {
        slots.push(json!({ "sourceOrder": source_order }));
        slots.len() - 1
    };

    let slot = slots[index]
        .as_object_mut()
        .ok_or_else(|| "Slot de geracao invalido.".to_string())?;
    ensure_slot_shape(slot, source_order);
    let additions = values
        .as_object()
        .ok_or_else(|| "Atualizacao de slot invalida.".to_string())?;
    for (key, value) in additions {
        slot.insert(key.clone(), value.clone());
    }
    production_obj.insert("updatedAt".to_string(), Value::String(now_string()));
    write_json(&production_path, &production)?;
    let _ = sync_project_snapshot_to_central_db(project_root);
    Ok(())
}

fn begin_slot_attempt(
    project_root: &Path,
    source_order: usize,
    command_type: &str,
    command_id: &str,
    prompt_text: &str,
    config: Value,
    image_media_id: Option<Value>,
    batch_id: Option<&str>,
) -> Result<String, String> {
    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    let production_obj = production
        .as_object_mut()
        .ok_or_else(|| "Arquivo de producao invalido.".to_string())?;
    let slots = production_obj
        .entry("generationSlots".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| "Lista de slots invalida.".to_string())?;
    let slot_index = slots.iter().position(|slot| {
        slot.get("sourceOrder").and_then(Value::as_u64) == Some(source_order as u64)
    });
    let index = if let Some(index) = slot_index {
        index
    } else {
        slots.push(json!({ "sourceOrder": source_order }));
        slots.len() - 1
    };
    let slot = slots[index]
        .as_object_mut()
        .ok_or_else(|| "Slot de geracao invalido.".to_string())?;
    ensure_slot_shape(slot, source_order);
    let attempt_id = new_attempt_id();
    let attempt_number = slot
        .get("attempts")
        .and_then(Value::as_array)
        .map(|attempts| attempts.len() + 1)
        .unwrap_or(1);
    let attempt = json!({
        "attemptId": attempt_id,
        "attemptNumber": attempt_number,
        "kind": attempt_kind_for_command_type(command_type),
        "commandType": command_type,
        "commandId": command_id,
        "batchId": batch_id,
        "state": "DISPATCHED",
        "remoteStatus": "COMMAND_DISPATCHED",
        "workflowId": Value::Null,
        "mediaId": Value::Null,
        "imageMediaId": image_media_id.clone().unwrap_or(Value::Null),
        "operationId": Value::Null,
        "thumbnailUrl": Value::Null,
        "remainingCredits": Value::Null,
        "prompt": prompt_text,
        "configuration": config,
        "createdAt": now_string(),
        "updatedAt": now_string(),
        "error": Value::Null
    });
    if let Some(attempts) = slot.get_mut("attempts").and_then(Value::as_array_mut) {
        attempts.push(attempt);
    }
    slot.insert("activeAttemptId".to_string(), json!(attempt_id.clone()));
    slot.insert("attemptCount".to_string(), json!(attempt_number));
    slot.insert("commandId".to_string(), json!(command_id));
    slot.insert("status".to_string(), json!("processing"));
    slot.insert("remoteStatus".to_string(), json!("COMMAND_DISPATCHED"));
    slot.insert("remoteUpdatedAt".to_string(), json!(now_string()));
    slot.insert("error".to_string(), Value::Null);
    slot.insert("prompt".to_string(), json!(prompt_text));
    if let Some(batch_id) = batch_id {
        slot.insert("batchId".to_string(), json!(batch_id));
    }
    if let Some(image_media_id) = image_media_id {
        slot.insert("imageMediaId".to_string(), image_media_id);
    }
    production_obj.insert("updatedAt".to_string(), json!(now_string()));
    write_json(&production_path, &production)?;
    let _ = sync_project_snapshot_to_central_db(project_root);
    Ok(attempt_id)
}

fn update_slot_attempt_fields(
    project_root: &Path,
    source_order: usize,
    values: Value,
) -> Result<(), String> {
    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    let production_obj = production
        .as_object_mut()
        .ok_or_else(|| "Arquivo de producao invalido.".to_string())?;
    let slots = production_obj
        .entry("generationSlots".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| "Lista de slots invalida.".to_string())?;
    let Some(index) = slots.iter().position(|slot| {
        slot.get("sourceOrder").and_then(Value::as_u64) == Some(source_order as u64)
    }) else {
        return Ok(());
    };
    let slot = slots[index]
        .as_object_mut()
        .ok_or_else(|| "Slot de geracao invalido.".to_string())?;
    ensure_slot_shape(slot, source_order);
    let active_attempt_id = slot
        .get("activeAttemptId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let additions = values
        .as_object()
        .ok_or_else(|| "Atualizacao de tentativa invalida.".to_string())?;
    for (key, value) in additions {
        slot.insert(key.clone(), value.clone());
    }
    if let Some(active_attempt_id) = active_attempt_id {
        if let Some(attempt) = slot
            .get_mut("attempts")
            .and_then(Value::as_array_mut)
            .and_then(|attempts| {
                attempts.iter_mut().find(|attempt| {
                    attempt.get("attemptId").and_then(Value::as_str)
                        == Some(active_attempt_id.as_str())
                })
            })
            .and_then(Value::as_object_mut)
        {
            for (key, value) in additions {
                match key.as_str() {
                    "status" => {}
                    "attemptState" => {
                        attempt.insert("state".to_string(), value.clone());
                    }
                    _ => {
                        attempt.insert(key.clone(), value.clone());
                    }
                }
            }
            attempt.insert("updatedAt".to_string(), json!(now_string()));
        }
    }
    if additions.contains_key("remoteStatus") {
        slot.insert("remoteUpdatedAt".to_string(), json!(now_string()));
    }
    production_obj.insert("updatedAt".to_string(), json!(now_string()));
    write_json(&production_path, &production)?;
    let _ = sync_project_snapshot_to_central_db(project_root);
    Ok(())
}

fn slot_requires_generation(slot: &Value) -> bool {
    slot.get("status")
        .and_then(Value::as_str)
        .is_none_or(|status| status != "ready")
}

fn slot_expected_asset_type(slot: &Value) -> &'static str {
    match slot.get("assetType").and_then(Value::as_str) {
        Some("video") => "video",
        _ => "image",
    }
}

fn slot_has_completed_local_asset(slot: &Value) -> bool {
    if slot.get("status").and_then(Value::as_str) != Some("ready") {
        return false;
    }
    match slot_expected_asset_type(slot) {
        "video" => slot.get("currentFileType").and_then(Value::as_str) == Some("video"),
        _ => slot
            .get("currentFileType")
            .and_then(Value::as_str)
            .is_some(),
    }
}

fn slot_source_order(slot: &Value) -> Option<usize> {
    slot.get("sourceOrder")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0)
}

fn slot_image_media_id(slot: &Value) -> Option<String> {
    slot.get("imageMediaId")
        .and_then(Value::as_str)
        .or_else(|| slot.get("mediaId").and_then(Value::as_str))
        .or_else(|| {
            slot.get("remoteUrl")
                .and_then(Value::as_str)
                .and_then(extract_media_id_from_url)
        })
        .map(str::to_string)
}

fn extract_media_id_from_url(url: &str) -> Option<&str> {
    let marker = "/image/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let end = rest.find(['?', '/']).unwrap_or(rest.len());
    let candidate = &rest[..end];
    if candidate.is_empty() {
        None
    } else {
        Some(candidate)
    }
}

fn build_animation_queue_items(
    prompts: &[Value],
    slots: &[Value],
    selected_orders: Option<&[usize]>,
) -> Vec<Value> {
    let selected = selected_orders.map(|orders| orders.iter().copied().collect::<Vec<usize>>());
    slots
        .iter()
        .filter_map(|slot| {
            let source_order = slot_source_order(slot)?;
            if selected
                .as_ref()
                .is_some_and(|orders| !orders.contains(&source_order))
            {
                return None;
            }
            let current_file_type = slot.get("currentFileType").and_then(Value::as_str);
            if current_file_type != Some("image") {
                return None;
            }
            let image_media_id = slot_image_media_id(slot)?;
            let prompt_text = prompts
                .iter()
                .find(|prompt| {
                    prompt.get("sourceOrder").and_then(Value::as_u64) == Some(source_order as u64)
                })
                .and_then(|prompt| prompt.get("prompt").and_then(Value::as_str))
                .unwrap_or("");
            Some(json!({
                "sourceOrder": source_order,
                "prompt": prompt_text,
                "imageMediaId": image_media_id
            }))
        })
        .collect()
}

fn build_queue_runtime_payload(queue: &GenerationQueueState) -> Value {
    let in_flight_orders: Vec<usize> = queue.in_flight.values().copied().collect();
    json!({
        "generationState": {
            "active": queue.active,
            "paused": queue.paused,
            "mode": queue.mode,
            "phase": queue.phase,
            "nextIndex": queue.next_index,
            "completedPrompts": queue.completed_assets.len(),
            "failedSlots": queue.failed_slots.iter().map(|(source_order, error)| {
                json!({ "sourceOrder": source_order, "error": error })
            }).collect::<Vec<Value>>(),
            "inFlight": in_flight_orders,
            "maxConcurrent": queue.max_concurrent,
            "targetSourceOrders": queue.target_source_orders,
            "queuedSourceOrders": queue
                .all_prompts
                .iter()
                .enumerate()
                .map(|(index, prompt)| prompt_source_order(prompt, index))
                .collect::<Vec<usize>>(),
            "currentBatchSourceOrders": queue.current_batch_source_orders,
            "remainingPrompts": queue.total_prompts.saturating_sub(queue.completed_assets.len())
        }
    })
}

fn prompt_source_order(prompt: &Value, fallback_index: usize) -> usize {
    prompt
        .get("sourceOrder")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0)
        .unwrap_or(fallback_index + 1)
}

fn normalize_prompt_entries(prompts: &[Value]) -> Vec<Value> {
    prompts
        .iter()
        .enumerate()
        .map(|(index, prompt)| {
            let mut normalized = prompt.clone();
            let source_order = prompt_source_order(prompt, index);
            if let Some(object) = normalized.as_object_mut() {
                object.insert("sourceOrder".to_string(), json!(source_order));
            }
            normalized
        })
        .collect()
}

fn select_prompt_subset(prompts: &[Value], source_orders: &[usize]) -> Vec<Value> {
    prompts
        .iter()
        .filter(|prompt| {
            let source_order = prompt
                .get("sourceOrder")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            source_order > 0 && source_orders.contains(&source_order)
        })
        .cloned()
        .collect()
}

fn source_orders_from_prompts(prompts: &[Value]) -> Vec<usize> {
    prompts
        .iter()
        .enumerate()
        .map(|(index, prompt)| prompt_source_order(prompt, index))
        .filter(|source_order| *source_order > 0)
        .collect()
}

fn initialize_image_to_video_batch(queue: &mut GenerationQueueState) {
    queue.phase = IMAGE_TO_VIDEO_PHASE_GENERATE.to_string();
    queue.current_batch_source_orders = source_orders_from_prompts(&queue.all_prompts)
        .into_iter()
        .take(queue.max_concurrent)
        .collect();
    queue.prompts = select_prompt_subset(&queue.all_prompts, &queue.current_batch_source_orders);
    queue.next_index = 0;
}

fn persist_generation_queue_state(queue: &GenerationQueueState) {
    if let Err(error) = update_production(&queue.project_root, build_queue_runtime_payload(queue)) {
        eprintln!(
            "[Queue] Falha ao persistir estado da geracao do projeto {}: {}",
            queue.local_project_id, error
        );
    }
    persist_queue_snapshot_to_ledger(queue);
}

fn advance_image_to_video_queue(queue: &mut GenerationQueueState) -> Result<bool, String> {
    let prompts = normalize_prompt_entries(
        &read_json(
            &queue
                .project_root
                .join("prompts")
                .join("ordered-prompts.json"),
        )?
        .get("prompts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default(),
    );
    let production = read_json(
        &queue
            .project_root
            .join(".flowcontent")
            .join("production.json"),
    )?;
    let slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if queue.phase == IMAGE_TO_VIDEO_PHASE_GENERATE {
        queue.phase = IMAGE_TO_VIDEO_PHASE_ANIMATE.to_string();
        queue.prompts = build_animation_queue_items(
            &prompts,
            &slots,
            Some(queue.current_batch_source_orders.as_slice()),
        );
        queue.next_index = 0;
        persist_generation_queue_state(queue);
        return Ok(false);
    }

    let finished_batch: HashSet<usize> =
        queue.current_batch_source_orders.iter().copied().collect();
    queue.all_prompts = queue
        .all_prompts
        .iter()
        .filter(|prompt| {
            let source_order = prompt
                .get("sourceOrder")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            !finished_batch.contains(&source_order)
        })
        .cloned()
        .collect();

    if queue.all_prompts.is_empty() {
        queue.active = false;
        queue.paused = false;
        queue.prompts.clear();
        queue.current_batch_source_orders.clear();
        queue.next_index = 0;
        persist_generation_queue_state(queue);
        return Ok(true);
    }

    initialize_image_to_video_batch(queue);
    persist_generation_queue_state(queue);
    Ok(false)
}

async fn download_asset_to_project(
    project_root: PathBuf,
    source_order: usize,
    asset_url: String,
    asset_extension: &str,
    slot_asset_type: &str,
) -> Result<PathBuf, String> {
    let downloads_dir = project_asset_output_dir(&project_root);
    fs::create_dir_all(&downloads_dir)
        .map_err(|error| format!("Nao foi possivel criar a pasta de downloads: {error}"))?;

    for ext in [
        "png", "jpg", "jpeg", "webp", "gif", "bmp", "mp4", "webm", "mov", "avi", "mkv",
    ] {
        let candidate = downloads_dir.join(format!("{:02}.{}", source_order, ext));
        if ext != asset_extension && candidate.exists() {
            let _ = fs::remove_file(candidate);
        }
    }

    let dest_path = downloads_dir.join(format!("{:02}.{}", source_order, asset_extension));
    let response = reqwest::get(&asset_url).await.map_err(|error| {
        format!(
            "Falha na conexao de download do slot {}: {error}",
            source_order
        )
    })?;
    if !response.status().is_success() {
        return Err(format!(
            "Falha no download do slot {}: status {}",
            source_order,
            response.status()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Falha ao obter bytes do slot {}: {error}", source_order))?;
    fs::write(&dest_path, bytes)
        .map_err(|error| format!("Nao foi possivel salvar o slot {}: {error}", source_order))?;

    update_generation_slot(
        &project_root,
        source_order,
        json!({
            "status": if slot_asset_type == "video" { "ready" } else { "image-ready" },
            "currentFileType": if slot_asset_type == "video" { "video" } else { "image" },
            "localPath": dest_path.to_string_lossy().to_string(),
            "remoteUrl": asset_url
        }),
    )?;

    Ok(dest_path)
}

fn detected_file_type_from_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => Some("image"),
        "mp4" | "webm" | "mov" | "avi" | "mkv" => Some("video"),
        _ => None,
    }
}

fn locate_downloaded_asset(
    project_root: &Path,
    source_order: usize,
) -> Option<(PathBuf, &'static str)> {
    let downloads_dir = project_asset_output_dir(project_root);
    if !downloads_dir.is_dir() {
        return None;
    }

    let prefix = format!("{:02}.", source_order);
    let mut best_match: Option<(PathBuf, &'static str)> = None;
    let mut best_is_video = false;

    for entry in fs::read_dir(&downloads_dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !file_name.starts_with(&prefix) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();
        let Some(file_type) = detected_file_type_from_extension(&ext) else {
            continue;
        };
        let is_video = file_type == "video";
        if best_match.is_none() || (!best_is_video && is_video) {
            best_match = Some((path, file_type));
            best_is_video = is_video;
        }
    }

    best_match
}

#[derive(Clone)]
struct DownloadedAssetInfo {
    source_order: usize,
    filename: String,
    full_path: String,
    file_type: &'static str,
    file_size: u64,
}

fn source_order_from_filename(filename: &str) -> Option<usize> {
    let digits: String = filename
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<usize>().ok()
    }
}

fn clear_downloaded_assets_for_orders(
    project_root: &Path,
    target_orders: &HashSet<usize>,
) -> Result<(), String> {
    if target_orders.is_empty() {
        return Ok(());
    }
    let downloads_dir = project_asset_output_dir(project_root);
    if !downloads_dir.is_dir() {
        return Ok(());
    }

    let entries = fs::read_dir(&downloads_dir)
        .map_err(|error| format!("Nao foi possivel ler a pasta de downloads: {error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let Some(source_order) = source_order_from_filename(filename) else {
            continue;
        };
        if !target_orders.contains(&source_order) {
            continue;
        }
        fs::remove_file(&path).map_err(|error| {
            format!("Nao foi possivel limpar o asset local {filename}: {error}")
        })?;
    }
    Ok(())
}

fn reset_generation_slots_for_orders(
    prompts: &[Value],
    mode: &str,
    existing_slots: &[Value],
    target_orders: &HashSet<usize>,
) -> Vec<Value> {
    let fresh_slots = build_generation_slots(prompts, mode);
    if target_orders.is_empty() {
        return fresh_slots;
    }

    let fresh_by_order: HashMap<usize, Value> = fresh_slots
        .into_iter()
        .filter_map(|slot| slot_source_order(&slot).map(|source_order| (source_order, slot)))
        .collect();

    prompts
        .iter()
        .enumerate()
        .filter_map(|(index, prompt)| {
            let source_order = prompt_source_order(prompt, index);
            if target_orders.contains(&source_order) {
                return fresh_by_order.get(&source_order).cloned();
            }

            existing_slots
                .iter()
                .find(|slot| slot_source_order(slot) == Some(source_order))
                .cloned()
                .or_else(|| fresh_by_order.get(&source_order).cloned())
        })
        .collect()
}

fn scan_downloaded_assets(project_root: &Path) -> Vec<DownloadedAssetInfo> {
    let downloads_dir = project_asset_output_dir(project_root);
    let mut downloaded_assets = Vec::new();
    if !downloads_dir.is_dir() {
        return downloaded_assets;
    }

    let Ok(entries) = fs::read_dir(&downloads_dir) else {
        return downloaded_assets;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        let Some(source_order) = source_order_from_filename(&filename) else {
            continue;
        };
        let ext = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();
        let Some(file_type) = detected_file_type_from_extension(&ext) else {
            continue;
        };
        let file_size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        downloaded_assets.push(DownloadedAssetInfo {
            source_order,
            filename,
            full_path: path.to_string_lossy().to_string(),
            file_type,
            file_size,
        });
    }

    downloaded_assets.sort_by(|left, right| {
        left.source_order
            .cmp(&right.source_order)
            .then_with(|| left.filename.cmp(&right.filename))
    });
    downloaded_assets
}

fn downloaded_assets_payload(downloads: &[DownloadedAssetInfo]) -> Vec<Value> {
    downloads
        .iter()
        .map(|asset| {
            json!({
                "filename": asset.filename,
                "fileType": asset.file_type,
                "fileSize": asset.file_size,
                "fullPath": asset.full_path
            })
        })
        .collect()
}

fn parse_source_orders(value: Option<&Value>) -> Vec<usize> {
    let mut seen = HashSet::new();
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_u64().map(|number| number as usize))
                .filter(|number| *number > 0)
                .filter(|number| seen.insert(*number))
                .collect::<Vec<usize>>()
        })
        .unwrap_or_default()
}

fn set_object_field(object: &mut serde_json::Map<String, Value>, key: &str, value: Value) -> bool {
    if object.get(key) == Some(&value) {
        return false;
    }
    object.insert(key.to_string(), value);
    true
}

fn persist_project_manifest_media_ready(project_root: &Path) {
    let manifest_path = project_root.join(".flowcontent").join("project.json");
    if let Ok(mut manifest) = read_json(&manifest_path) {
        if let Some(obj) = manifest.as_object_mut() {
            obj.insert("remoteMediaStoredLocally".to_string(), json!(true));
            obj.insert("updatedAt".to_string(), json!(now_string()));
        }
        let _ = write_json(&manifest_path, &manifest);
    }
}

fn mark_project_media_ready(project_root: &Path) {
    let _ = update_production(
        project_root,
        json!({
            "stage": "READY_FOR_FLOW",
            "remoteMediaStoredLocally": true
        }),
    );
    persist_project_manifest_media_ready(project_root);
}

fn reconcile_production_with_downloads(
    project_root: &Path,
    production: &mut Value,
    downloads: &[DownloadedAssetInfo],
) -> bool {
    let mut changed = false;
    let mut finalized_orders = HashSet::new();
    let downloaded_by_order: HashMap<usize, &DownloadedAssetInfo> = downloads
        .iter()
        .map(|asset| (asset.source_order, asset))
        .collect();

    let stage_is_generating =
        production.get("stage").and_then(Value::as_str) == Some("GENERATING_ASSETS");
    let Some(production_obj) = production.as_object_mut() else {
        return false;
    };

    let slot_snapshot = {
        let Some(slots) = production_obj
            .entry("generationSlots".to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
        else {
            return false;
        };

        for slot in slots.iter_mut() {
            let Some(source_order) = slot_source_order(slot) else {
                continue;
            };
            let Some(downloaded) = downloaded_by_order.get(&source_order) else {
                continue;
            };
            let expected_type = slot_expected_asset_type(slot);
            let Some(slot_obj) = slot.as_object_mut() else {
                continue;
            };

            let is_final_asset = expected_type != "video" || downloaded.file_type == "video";
            let resolved_status = if is_final_asset {
                "ready"
            } else {
                "image-ready"
            };

            changed |= set_object_field(
                slot_obj,
                "currentFileType",
                Value::String(downloaded.file_type.to_string()),
            );
            changed |= set_object_field(
                slot_obj,
                "localPath",
                Value::String(downloaded.full_path.clone()),
            );
            changed |= set_object_field(
                slot_obj,
                "status",
                Value::String(resolved_status.to_string()),
            );
            if is_final_asset {
                changed |= set_object_field(slot_obj, "error", Value::Null);
                finalized_orders.insert(source_order);
            }
        }

        slots.clone()
    };

    let should_track_generation_state =
        stage_is_generating || production_obj.contains_key("generationState");
    if should_track_generation_state {
        let all_slot_orders: Vec<usize> =
            slot_snapshot.iter().filter_map(slot_source_order).collect();
        let state_value = production_obj
            .entry("generationState".to_string())
            .or_insert_with(|| json!({}));
        let Some(state_obj) = state_value.as_object_mut() else {
            return changed;
        };

        let mut target_source_orders = parse_source_orders(state_obj.get("targetSourceOrders"));
        if target_source_orders.is_empty() {
            target_source_orders = all_slot_orders.clone();
        }
        changed |= set_object_field(
            state_obj,
            "targetSourceOrders",
            json!(target_source_orders.clone()),
        );
        let target_order_set: HashSet<usize> = target_source_orders.iter().copied().collect();
        let normalized_failed_slots: Vec<Value> = state_obj
            .get("failedSlots")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let source_order = item.get("sourceOrder").and_then(Value::as_u64)? as usize;
                        if source_order == 0 {
                            return None;
                        }
                        Some(json!({
                            "sourceOrder": source_order,
                            "error": item.get("error").and_then(Value::as_str).unwrap_or("Erro desconhecido")
                        }))
                    })
                    .collect::<Vec<Value>>()
            })
            .unwrap_or_default();
        let completed_prompts = target_source_orders
            .iter()
            .filter(|order| finalized_orders.contains(order))
            .count();
        let in_flight_orders: Vec<usize> = parse_source_orders(state_obj.get("inFlight"))
            .into_iter()
            .filter(|order| target_order_set.contains(order) && !finalized_orders.contains(order))
            .collect();
        let remaining_prompts = target_source_orders.len().saturating_sub(completed_prompts);
        let existing_active = state_obj
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let active_now = existing_active && remaining_prompts > 0;
        let existing_next_index = state_obj
            .get("nextIndex")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let next_index = if remaining_prompts == 0 {
            target_source_orders.len()
        } else {
            existing_next_index.max(completed_prompts + in_flight_orders.len())
        };

        changed |= set_object_field(state_obj, "completedPrompts", json!(completed_prompts));
        changed |= set_object_field(state_obj, "failedSlots", json!(normalized_failed_slots));
        changed |= set_object_field(state_obj, "inFlight", json!(in_flight_orders));
        changed |= set_object_field(state_obj, "remainingPrompts", json!(remaining_prompts));
        changed |= set_object_field(state_obj, "nextIndex", json!(next_index));
        changed |= set_object_field(
            state_obj,
            "maxConcurrent",
            json!(normalized_queue_concurrency(
                state_obj
                    .get("maxConcurrent")
                    .and_then(Value::as_u64)
                    .unwrap_or(2) as usize
            )),
        );
        if remaining_prompts == 0 {
            changed |= set_object_field(state_obj, "active", json!(false));
            changed |= set_object_field(state_obj, "paused", json!(false));
            changed |= set_object_field(production_obj, "stage", json!("READY_FOR_FLOW"));
            changed |= set_object_field(production_obj, "remoteMediaStoredLocally", json!(true));
            persist_project_manifest_media_ready(project_root);
        } else if active_now != existing_active {
            changed |= set_object_field(state_obj, "active", json!(active_now));
        }
        changed |= set_object_field(
            production_obj,
            "generationTotalPrompts",
            json!(target_source_orders.len()),
        );
    }

    changed
}

fn reconcile_live_generation_queue(queue: &mut GenerationQueueState) -> Result<bool, String> {
    let production_path = queue
        .project_root
        .join(".flowcontent")
        .join("production.json");
    let mut changed = false;
    let mut production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    let downloads = scan_downloaded_assets(&queue.project_root);
    if reconcile_production_with_downloads(&queue.project_root, &mut production, &downloads) {
        write_json(&production_path, &production)?;
        changed = true;
    }

    let slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut target_orders = if queue.target_source_orders.is_empty() {
        parse_source_orders(
            production
                .get("generationState")
                .and_then(|value| value.get("targetSourceOrders")),
        )
    } else {
        queue.target_source_orders.clone()
    };
    if target_orders.is_empty() {
        target_orders = slots.iter().filter_map(slot_source_order).collect();
    }

    let completed_orders: HashSet<usize> = slots
        .iter()
        .filter(|slot| slot_has_completed_local_asset(slot))
        .filter_map(slot_source_order)
        .filter(|order| target_orders.contains(order))
        .collect();
    let previous_completed = queue.completed_assets.len();
    queue.completed_assets = target_orders
        .iter()
        .filter(|order| completed_orders.contains(order))
        .map(|order| json!({ "sourceOrder": order }))
        .collect();
    if queue.completed_assets.len() != previous_completed {
        changed = true;
    }

    let previous_in_flight = queue.in_flight.len();
    queue
        .in_flight
        .retain(|_, source_order| !completed_orders.contains(source_order));
    if queue.in_flight.len() != previous_in_flight {
        changed = true;
    }

    let previous_failed = queue.failed_slots.len();
    queue
        .failed_slots
        .retain(|(source_order, _)| !completed_orders.contains(source_order));
    if queue.failed_slots.len() != previous_failed {
        changed = true;
    }

    if queue.target_source_orders.is_empty() && !target_orders.is_empty() {
        queue.target_source_orders = target_orders.clone();
        changed = true;
    }
    if queue.total_prompts < target_orders.len() {
        queue.total_prompts = target_orders.len();
        changed = true;
    }

    if queue.completed_assets.len() >= queue.total_prompts {
        if queue.active || !queue.in_flight.is_empty() || queue.paused {
            queue.active = false;
            queue.paused = false;
            queue.in_flight.clear();
            queue.next_index = queue.prompts.len();
            changed = true;
        }
    } else if queue.active && queue.next_index >= queue.prompts.len() && queue.in_flight.is_empty()
    {
        let pending_prompts: Vec<Value> = queue
            .prompts
            .iter()
            .filter(|prompt| {
                let source_order = prompt
                    .get("sourceOrder")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                source_order > 0 && !completed_orders.contains(&source_order)
            })
            .cloned()
            .collect();
        if !pending_prompts.is_empty() {
            queue.prompts = pending_prompts;
            queue.next_index = 0;
            changed = true;
        }
    }

    if changed {
        persist_generation_queue_state(queue);
    }

    Ok(changed)
}

fn local_generation_progress_snapshot(
    queue: &GenerationQueueState,
) -> Result<(usize, usize, Vec<Value>, usize), String> {
    let production_path = queue
        .project_root
        .join(".flowcontent")
        .join("production.json");
    let mut production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    let downloads = scan_downloaded_assets(&queue.project_root);
    if reconcile_production_with_downloads(&queue.project_root, &mut production, &downloads) {
        write_json(&production_path, &production)?;
    }
    let slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let generation_state = production
        .get("generationState")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut target_orders = if queue.target_source_orders.is_empty() {
        parse_source_orders(generation_state.get("targetSourceOrders"))
    } else {
        queue.target_source_orders.clone()
    };
    if target_orders.is_empty() {
        target_orders = slots.iter().filter_map(slot_source_order).collect();
    }
    let target_set: HashSet<usize> = target_orders.iter().copied().collect();
    let completed_prompts = slots
        .iter()
        .filter(|slot| slot_has_completed_local_asset(slot))
        .filter_map(slot_source_order)
        .filter(|order| target_set.contains(order))
        .count();
    let failed_slots = generation_state
        .get("failedSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let next_index = generation_state
        .get("nextIndex")
        .and_then(Value::as_u64)
        .unwrap_or(queue.next_index as u64) as usize;
    Ok((
        target_orders.len(),
        completed_prompts,
        failed_slots,
        next_index,
    ))
}

fn image_mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

fn video_preview_path(project_root: &Path, source_order: usize) -> PathBuf {
    project_root
        .join(".flowcontent")
        .join("video-previews")
        .join(format!("{:02}.jpg", source_order))
}

fn ensure_video_preview_image(project_root: &Path, source_order: usize) -> Result<PathBuf, String> {
    let (video_path, file_type) =
        locate_downloaded_asset(project_root, source_order).ok_or_else(|| {
            format!(
                "Nenhum arquivo encontrado em downloads para o slot {}.",
                source_order
            )
        })?;
    if file_type != "video" {
        return Err(format!(
            "O slot {} ainda nao possui um video salvo.",
            source_order
        ));
    }
    let preview_path = video_preview_path(project_root, source_order);
    if preview_path.is_file() {
        return Ok(preview_path);
    }

    let preview_dir = preview_path
        .parent()
        .ok_or_else(|| "Pasta de preview invalida.".to_string())?;
    fs::create_dir_all(preview_dir)
        .map_err(|error| format!("Nao foi possivel criar a pasta de previews: {error}"))?;

    let output = Command::new("ffmpeg")
        .arg("-y")
        .arg("-ss")
        .arg("0")
        .arg("-i")
        .arg(&video_path)
        .arg("-frames:v")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        .arg(&preview_path)
        .output()
        .map_err(|error| format!("Nao foi possivel iniciar ffmpeg: {error}"))?;

    if !output.status.success() || !preview_path.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Falha ao gerar thumbnail do video: {}",
            stderr.trim()
        ));
    }

    Ok(preview_path)
}

impl FlowBridgeState {
    fn new() -> Self {
        Self {
            token: load_or_create_bridge_token(),
            status: Arc::new(Mutex::new(FlowBridgeStatus {
                server_ready: true,
                ..FlowBridgeStatus::default()
            })),
            pending_commands: Arc::new(Mutex::new(HashMap::new())),
            acknowledged_commands: Arc::new(Mutex::new(HashSet::new())),
            command_results: Arc::new(Mutex::new(vec![])),
            ws_sender: Arc::new(Mutex::new(None)),
            app_handle: Arc::new(Mutex::new(None)),
            generation_queue: Arc::new(Mutex::new(None)),
        }
    }
}

fn queue_bridge_command(bridge: &FlowBridgeState, command: Value) -> Result<String, String> {
    let command_id = command
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Comando Flow sem identificador.".to_string())?
        .to_string();
    let pending_count = {
        let mut pending = bridge
            .pending_commands
            .lock()
            .map_err(|_| "Nao foi possivel acessar a fila da ponte Flow.".to_string())?;
        pending.insert(command_id.clone(), command.clone());
        pending.len()
    };
    if let Ok(mut acknowledged) = bridge.acknowledged_commands.lock() {
        acknowledged.remove(&command_id);
    }
    if let Ok(mut status) = bridge.status.lock() {
        status.pending_command = if pending_count > 0 {
            Some(format!("{} comando(s) em voo", pending_count))
        } else {
            None
        };
        status.last_command_error = None;
    }
    if let Some(local_project_id) = command.get("localProjectId").and_then(Value::as_str) {
        if let Ok(queue_guard) = bridge.generation_queue.lock() {
            if let Some(queue) = queue_guard.as_ref() {
                if queue.local_project_id == local_project_id {
                    record_generation_command(
                        &queue.project_root,
                        &queue.local_project_id,
                        &queue.flow_project_id,
                        &command,
                        "queued",
                        None,
                    );
                }
            }
        }
    }

    // Push to WebSocket channel instantly if connected!
    if let Ok(sender_guard) = bridge.ws_sender.lock() {
        if let Some(sender) = &*sender_guard {
            let payload = json!({
                "ok": true,
                "command": command
            });
            if let Ok(json_str) = serde_json::to_string(&payload) {
                let _ = sender.send(json_str);
                println!(
                    "[Bridge] Comando enviado instantaneamente via WebSocket: {:?}",
                    command_id
                );
            }
        }
    }

    Ok(command_id)
}

fn normalized_queue_concurrency(requested: usize) -> usize {
    requested.max(1).min(FLOW_MAX_CONCURRENT)
}

fn project_asset_output_dir(project_root: &Path) -> PathBuf {
    let production_path = project_root.join(".flowcontent").join("production.json");
    if let Ok(production) = read_json(&production_path) {
        if let Some(path) = production
            .get("assetOutputDir")
            .and_then(Value::as_str)
            .map(PathBuf::from)
        {
            return path;
        }
    }
    project_root.join("downloads")
}

fn load_or_create_bridge_token() -> String {
    let token = Uuid::new_v4().to_string();
    let Some(app_data) = env::var_os("APPDATA") else {
        return token;
    };
    let app_data = PathBuf::from(app_data).join("com.flowcontent.auto");
    let token_path = app_data.join("bridge-token");
    if let Ok(saved_token) = fs::read_to_string(&token_path) {
        let saved_token = saved_token.trim();
        if !saved_token.is_empty() {
            return saved_token.to_string();
        }
    }
    if fs::create_dir_all(app_data).is_ok() {
        let _ = fs::write(token_path, &token);
    }
    token
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    app_data_dir: PathBuf,
    documents_dir: Option<PathBuf>,
    workspace_root: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssemblyAiStatus {
    configured: bool,
    key_count: usize,
    masked_keys: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectEntry {
    local_project_id: String,
    title: String,
    flow_project_id: Option<String>,
    project_root: PathBuf,
    manifest_path: PathBuf,
    last_opened_at: String,
}

#[derive(Serialize, Deserialize)]
struct Registry {
    version: u8,
    projects: Vec<ProjectEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    local_project_id: String,
    title: String,
    flow_project_id: Option<String>,
    project_root: PathBuf,
    asset_output_dir: PathBuf,
    stage: String,
    asset_count: usize,
    prompt_count: usize,
    caption_srt_path: Option<PathBuf>,
    asset_srt_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    updated_at: String,
}

fn now_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
fn http_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type, X-FlowContent-Bridge\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
fn read_http_request(stream: &mut std::net::TcpStream) -> Result<String, String> {
    use std::io::Read;

    const MAX_REQUEST_SIZE: usize = 1_048_576;
    let mut request = Vec::new();
    let mut expected_size = None;
    let mut buffer = [0_u8; 8_192];

    loop {
        let size = stream
            .read(&mut buffer)
            .map_err(|error| format!("Nao foi possivel ler a requisicao da ponte: {error}"))?;
        if size == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..size]);
        if request.len() > MAX_REQUEST_SIZE {
            return Err("Requisicao da ponte excedeu o limite permitido.".to_string());
        }

        if expected_size.is_none() {
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .skip(1)
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                expected_size = Some(header_end + 4 + content_length);
            }
        }

        if expected_size.is_some_and(|expected| request.len() >= expected) {
            request.truncate(expected_size.unwrap_or(request.len()));
            break;
        }
    }

    String::from_utf8(request).map_err(|_| "Requisicao da ponte nao esta em UTF-8.".to_string())
}

#[cfg(test)]
fn handle_bridge_request(mut stream: std::net::TcpStream, bridge: &FlowBridgeState) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let Ok(request) = read_http_request(&mut stream) else {
        http_response(&mut stream, "400 Bad Request", r#"{"ok":false}"#);
        return;
    };
    let Some((headers, body)) = request.split_once("\r\n\r\n") else {
        http_response(&mut stream, "400 Bad Request", r#"{"ok":false}"#);
        return;
    };
    let request_line = headers.lines().next().unwrap_or_default();
    if request_line.starts_with("OPTIONS ") {
        http_response(&mut stream, "204 No Content", "");
        return;
    }
    if !request_line.starts_with("POST /heartbeat ")
        && !request_line.starts_with("POST /command-result ")
    {
        http_response(&mut stream, "404 Not Found", r#"{"ok":false}"#);
        return;
    }

    let authorized = headers.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.trim().eq_ignore_ascii_case("x-flowcontent-bridge") && value.trim() == bridge.token
        })
    });
    if !authorized {
        http_response(&mut stream, "401 Unauthorized", r#"{"ok":false}"#);
        return;
    }

    let Ok(payload) = serde_json::from_str::<Value>(body) else {
        http_response(&mut stream, "400 Bad Request", r#"{"ok":false}"#);
        return;
    };
    if request_line.starts_with("POST /command-result ") {
        let command_id = payload.get("id").and_then(Value::as_str);
        let command_error = if payload.get("ok").and_then(Value::as_bool) == Some(false) {
            payload
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        if let (Some(command_id), Ok(mut pending)) = (command_id, bridge.pending_commands.lock()) {
            pending.remove(command_id);
        }
        if let Ok(mut results) = bridge.command_results.lock() {
            results.push(payload);
        }
        if let Ok(mut status) = bridge.status.lock() {
            status.pending_command = None;
            status.last_command_error = command_error;
        }
        http_response(&mut stream, "200 OK", r#"{"ok":true}"#);
        return;
    }

    if let Ok(mut status) = bridge.status.lock() {
        status.extension_installed = true;
        status.extension_connected = true;
        status.flow_page_detected = payload
            .get("pageDetected")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        status.flow_url = payload
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string);
        status.project_id = payload
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        status.page_title = payload
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string);
        status.last_heartbeat_ms = Some(now_millis());
    }
    let observed_project_id = payload
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let recovered_result = if let Some(project_id) = observed_project_id {
        bridge.pending_commands.lock().ok().and_then(|mut pending| {
            let create_command = pending.iter().find_map(|(id, command)| {
                (command.get("type").and_then(Value::as_str) == Some("CREATE_PROJECT"))
                    .then(|| (id.clone(), command.clone()))
            })?;
            pending.remove(&create_command.0);
            let command = create_command.1;
            let result = json!({
                "id": command.get("id")?,
                "type": "CREATE_PROJECT",
                "ok": true,
                "localProjectId": command.get("localProjectId")?,
                "projectId": project_id
            });
            Some(result)
        })
    } else {
        None
    };
    if let Some(result) = recovered_result {
        if let Ok(mut results) = bridge.command_results.lock() {
            results.push(result);
        }
        if let Ok(mut status) = bridge.status.lock() {
            status.pending_command = None;
            status.last_command_error = None;
        }
    }
    let command = bridge
        .pending_commands
        .lock()
        .ok()
        .and_then(|pending| pending.values().next().cloned());
    http_response(
        &mut stream,
        "200 OK",
        &serde_json::to_string(&json!({ "ok": true, "command": command }))
            .unwrap_or_else(|_| r#"{"ok":true}"#.to_string()),
    );
}

fn start_bridge_server(bridge: FlowBridgeState) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        run_websocket_server(bridge).await;
    });
    Ok(())
}

/// Dispatch prompts while the queue has room, keeping up to `max_concurrent` commands in flight.
fn pump_generation_queue(bridge: &FlowBridgeState) {
    loop {
        let dispatch = {
            let mut queue_guard = match bridge.generation_queue.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            let Some(queue) = queue_guard.as_mut() else {
                return;
            };
            if !queue.active || queue.paused || queue.in_flight.len() >= queue.max_concurrent {
                persist_generation_queue_state(queue);
                return;
            }
            let Some(prompt_item) = queue.prompts.get(queue.next_index).cloned() else {
                persist_generation_queue_state(queue);
                return;
            };
            let source_order = prompt_item
                .get("sourceOrder")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .filter(|value| *value > 0);
            let Some(source_order) = source_order else {
                queue.next_index += 1;
                eprintln!("[Queue] Ignorando prompt sem sourceOrder valido.");
                persist_generation_queue_state(queue);
                continue;
            };
            let prompt_text = prompt_item
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let cmd_type = match queue.mode.as_str() {
                "IMAGE" => "GENERATE_IMAGE",
                "VIDEO" => "GENERATE_VIDEO",
                "IMAGE_TO_VIDEO" => {
                    if queue.phase == IMAGE_TO_VIDEO_PHASE_ANIMATE {
                        "ANIMATE_IMAGE"
                    } else {
                        "GENERATE_IMAGE"
                    }
                }
                "ANIMATE_IMAGES" => "ANIMATE_IMAGE",
                _ => "GENERATE_IMAGE",
            };
            let command_id = Uuid::new_v4().to_string();
            let batch_id = Uuid::new_v4().to_string();
            let command = json!({
                "id": command_id,
                "batchId": batch_id,
                "type": cmd_type,
                "localProjectId": queue.local_project_id,
                "projectId": queue.flow_project_id,
                "prompt": prompt_text,
                "sourceOrder": source_order,
                "imageModel": queue.image_model,
                "videoModel": queue.video_model,
                "i2vModel": queue.i2v_model,
                "imageAspectRatio": queue.image_aspect_ratio,
                "videoAspectRatio": queue.video_aspect_ratio,
                "imageMediaId": prompt_item.get("imageMediaId").cloned().unwrap_or(Value::Null)
            });
            queue.next_index += 1;
            queue.in_flight.insert(command_id.clone(), source_order);
            persist_generation_queue_state(queue);
            Some((
                command_id,
                command,
                queue.project_root.clone(),
                source_order,
                queue.total_prompts,
            ))
        };

        let Some((command_id, command, project_root, source_order, total_prompts)) = dispatch
        else {
            return;
        };

        let cmd_type = command.get("type").and_then(Value::as_str).unwrap_or("");
        println!(
            "[Queue] Enviando slot {} de {} (tipo: {}, id={})...",
            source_order, total_prompts, cmd_type, command_id
        );
        let _ = begin_slot_attempt(
            &project_root,
            source_order,
            cmd_type,
            &command_id,
            command.get("prompt").and_then(Value::as_str).unwrap_or(""),
            json!({
                "imageModel": command.get("imageModel").cloned().unwrap_or(Value::Null),
                "videoModel": command.get("videoModel").cloned().unwrap_or(Value::Null),
                "i2vModel": command.get("i2vModel").cloned().unwrap_or(Value::Null),
                "imageAspectRatio": command.get("imageAspectRatio").cloned().unwrap_or(Value::Null),
                "videoAspectRatio": command.get("videoAspectRatio").cloned().unwrap_or(Value::Null)
            }),
            command.get("imageMediaId").cloned(),
            command.get("batchId").and_then(Value::as_str),
        );
        if let Err(error) = queue_bridge_command(bridge, command) {
            eprintln!("[Queue] Falha ao enviar slot {}: {}", source_order, error);
            if let Ok(mut queue_guard) = bridge.generation_queue.lock() {
                if let Some(queue) = queue_guard.as_mut() {
                    queue.in_flight.remove(&command_id);
                    if queue.next_index > 0 {
                        queue.next_index -= 1;
                    }
                    queue.failed_slots.push((source_order, error.clone()));
                    persist_generation_queue_state(queue);
                }
            }
            let _ = update_slot_attempt_fields(
                &project_root,
                source_order,
                json!({
                    "status": "failed",
                    "error": error,
                    "remoteStatus": "DISPATCH_FAILED",
                    "attemptState": "FAILED"
                }),
            );
            return;
        }
    }
}

async fn run_websocket_server(bridge: FlowBridgeState) {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let ports = [9999u16, 10000, 10001];
    let mut listener_opt = None;
    let mut active_port = 9999;
    for port in ports {
        match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
            Ok(l) => {
                listener_opt = Some(l);
                active_port = port;
                break;
            }
            Err(e) => {
                println!("[Bridge] Porta {} ocupada: {}", port, e);
            }
        }
    }

    let listener = match listener_opt {
        Some(l) => l,
        None => {
            eprintln!("[Bridge] Todas as portas estão ocupadas. Bridge desativado.");
            return;
        }
    };

    println!(
        "[Bridge] Servidor WebSocket rodando em ws://127.0.0.1:{}",
        active_port
    );

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("[Bridge] Conexão recebida de {}", addr);
                let bridge_clone = bridge.clone();

                tokio::spawn(async move {
                    let ws = match accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            eprintln!("[Bridge] Erro no handshake WebSocket: {}", e);
                            return;
                        }
                    };

                    let (mut ws_sender, mut ws_receiver) = ws.split();
                    let (outbound_tx, mut outbound_rx) =
                        tokio::sync::mpsc::unbounded_channel::<String>();

                    // Register the WebSocket sender
                    if let Ok(mut sender_guard) = bridge_clone.ws_sender.lock() {
                        *sender_guard = Some(outbound_tx);
                    }
                    if let Ok(mut status) = bridge_clone.status.lock() {
                        status.extension_connected = true;
                        status.extension_installed = true;
                    }

                    // Task: forward outbound messages to WebSocket
                    let send_task = tokio::spawn(async move {
                        while let Some(msg) = outbound_rx.recv().await {
                            if ws_sender.send(Message::Text(msg)).await.is_err() {
                                break;
                            }
                        }
                    });

                    // Resend any in-flight command after reconnect.
                    if let Ok(pending_guard) = bridge_clone.pending_commands.lock() {
                        if let Ok(sender_guard) = bridge_clone.ws_sender.lock() {
                            if let Some(sender) = &*sender_guard {
                                for command in pending_guard.values() {
                                    let payload = json!({
                                        "ok": true,
                                        "command": command
                                    });
                                    if let Ok(json_str) = serde_json::to_string(&payload) {
                                        let _ = sender.send(json_str);
                                        println!(
                                            "[Bridge] Reenviou comando pendente no reconnect: {:?}",
                                            command.get("id")
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Receive loop
                    while let Some(msg_res) = ws_receiver.next().await {
                        match msg_res {
                            Ok(Message::Text(text)) => {
                                // We receive either:
                                // 1. FLOWCONTENT_HEARTBEAT
                                // 2. FLOWCONTENT_COMMAND_RESULT
                                if let Ok(payload) = serde_json::from_str::<Value>(&text) {
                                    let msg_type =
                                        payload.get("type").and_then(Value::as_str).unwrap_or("");
                                    if msg_type == "FLOWCONTENT_HEARTBEAT" {
                                        let data = payload.get("payload").unwrap_or(&payload);
                                        if let Ok(mut status) = bridge_clone.status.lock() {
                                            status.extension_installed = true;
                                            status.extension_connected = true;
                                            status.flow_page_detected = data
                                                .get("pageDetected")
                                                .and_then(Value::as_bool)
                                                .unwrap_or(false);
                                            status.flow_url = data
                                                .get("url")
                                                .and_then(Value::as_str)
                                                .map(str::to_string);
                                            status.project_id = data
                                                .get("projectId")
                                                .and_then(Value::as_str)
                                                .map(str::to_string);
                                            status.page_title = data
                                                .get("title")
                                                .and_then(Value::as_str)
                                                .map(str::to_string);
                                            status.last_heartbeat_ms = Some(now_millis());
                                        }
                                        // Check if we recovered/completed a CREATE_PROJECT implicitly
                                        let observed_project_id = data
                                            .get("projectId")
                                            .and_then(Value::as_str)
                                            .map(str::to_string);
                                        let recovered_result = if let Some(project_id) =
                                            observed_project_id
                                        {
                                            bridge_clone.pending_commands.lock().ok().and_then(|mut pending| {
                                                let create_command = pending
                                                    .iter()
                                                    .find_map(|(id, command)| {
                                                        (command.get("type").and_then(Value::as_str) == Some("CREATE_PROJECT"))
                                                            .then(|| (id.clone(), command.clone()))
                                                    })?;
                                                pending.remove(&create_command.0);
                                                Some(json!({
                                                    "id": create_command.1.get("id")?,
                                                    "type": "CREATE_PROJECT",
                                                    "ok": true,
                                                    "localProjectId": create_command.1.get("localProjectId")?,
                                                    "projectId": project_id
                                                }))
                                            })
                                        } else {
                                            None
                                        };
                                        if let Some(result) = recovered_result {
                                            let app_handle_opt = bridge_clone
                                                .app_handle
                                                .lock()
                                                .ok()
                                                .and_then(|guard| guard.clone());
                                            let applied_locally = match app_handle_opt.as_ref() {
                                                Some(app) => {
                                                    match apply_create_project_result(app, &result)
                                                    {
                                                        Ok(value) => value,
                                                        Err(error) => {
                                                            eprintln!("[Bridge] Falha ao persistir CREATE_PROJECT: {}", error);
                                                            false
                                                        }
                                                    }
                                                }
                                                None => false,
                                            };
                                            if !applied_locally {
                                                if let Ok(mut results) =
                                                    bridge_clone.command_results.lock()
                                                {
                                                    results.push(result);
                                                }
                                            }
                                            if let Ok(mut status) = bridge_clone.status.lock() {
                                                let pending_len = bridge_clone
                                                    .pending_commands
                                                    .lock()
                                                    .ok()
                                                    .map(|pending| pending.len())
                                                    .unwrap_or(0);
                                                status.pending_command = if pending_len > 0 {
                                                    Some(format!(
                                                        "{} comando(s) em voo",
                                                        pending_len
                                                    ))
                                                } else {
                                                    None
                                                };
                                                status.last_command_error = None;
                                            }
                                        }
                                    } else if msg_type == "FLOWCONTENT_PROGRESS" {
                                        let data =
                                            payload.get("payload").unwrap_or(&payload).clone();
                                        let source_order = data
                                            .get("sourceOrder")
                                            .and_then(Value::as_u64)
                                            .unwrap_or(0)
                                            as usize;
                                        let status_name = data
                                            .get("status")
                                            .and_then(Value::as_str)
                                            .unwrap_or("");
                                        let app_handle_opt = bridge_clone
                                            .app_handle
                                            .lock()
                                            .ok()
                                            .and_then(|guard| guard.clone());
                                        if source_order > 0 {
                                            if let Some(app) = app_handle_opt.as_ref() {
                                                use tauri::Emitter;
                                                let _ = app
                                                    .emit("flowcontent-slot-updated", data.clone());
                                            }
                                            if let Some(app_handle) = app_handle_opt.as_ref() {
                                                if let Ok(registry) = read_registry(app_handle) {
                                                    let local_project_id = data
                                                        .get("localProjectId")
                                                        .and_then(Value::as_str)
                                                        .unwrap_or("");
                                                    if let Some(entry) =
                                                        registry.projects.iter().find(|p| {
                                                            p.local_project_id == local_project_id
                                                        })
                                                    {
                                                        let mut fields = serde_json::Map::new();
                                                        fields.insert(
                                                            "status".to_string(),
                                                            json!("processing"),
                                                        );
                                                        for key in [
                                                            "mediaId",
                                                            "imageMediaId",
                                                            "workflowId",
                                                            "batchId",
                                                            "operationId",
                                                            "thumbnailUrl",
                                                            "remainingCredits",
                                                        ] {
                                                            if let Some(value) = data.get(key) {
                                                                fields.insert(
                                                                    key.to_string(),
                                                                    value.clone(),
                                                                );
                                                            }
                                                        }
                                                        if let Some(remote_status) =
                                                            data.get("remoteStatus")
                                                        {
                                                            fields.insert(
                                                                "remoteStatus".to_string(),
                                                                remote_status.clone(),
                                                            );
                                                        }
                                                        let _ = update_slot_attempt_fields(
                                                            &entry.project_root,
                                                            source_order,
                                                            Value::Object(fields),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        if source_order > 0 && status_name == "IMAGE_READY" {
                                            if let Some(app_handle) = app_handle_opt {
                                                let registry = read_registry(&app_handle);
                                                if let Ok(registry) = registry {
                                                    let local_project_id = data
                                                        .get("localProjectId")
                                                        .and_then(Value::as_str)
                                                        .unwrap_or("");
                                                    if let Some(entry) =
                                                        registry.projects.iter().find(|p| {
                                                            p.local_project_id == local_project_id
                                                        })
                                                    {
                                                        let project_root =
                                                            entry.project_root.clone();
                                                        let asset_url = data
                                                            .get("url")
                                                            .and_then(Value::as_str)
                                                            .unwrap_or("")
                                                            .to_string();
                                                        if !asset_url.is_empty() {
                                                            let media_id = data
                                                                .get("mediaId")
                                                                .and_then(Value::as_str)
                                                                .map(str::to_string);
                                                            let app_for_event = app_handle.clone();
                                                            let local_project_id_for_event =
                                                                local_project_id.to_string();
                                                            let _ = update_slot_attempt_fields(
                                                                &project_root,
                                                                source_order,
                                                                json!({
                                                                    "attemptState": "SUCCESSFUL",
                                                                    "remoteStatus": "REMOTE_IMAGE_READY",
                                                                    "mediaId": media_id,
                                                                    "remoteUrl": asset_url,
                                                                    "workflowId": data.get("workflowId").cloned().unwrap_or(Value::Null),
                                                                    "batchId": data.get("batchId").cloned().unwrap_or(Value::Null)
                                                                }),
                                                            );
                                                            tokio::spawn(async move {
                                                                match download_asset_to_project(
                                                                    project_root.clone(),
                                                                    source_order,
                                                                    asset_url.clone(),
                                                                    "png",
                                                                    "image",
                                                                )
                                                                .await
                                                                {
                                                                    Ok(path) => {
                                                                        let local_path = path
                                                                            .to_string_lossy()
                                                                            .to_string();
                                                                        let _ = update_slot_attempt_fields(
                                                                            &project_root,
                                                                            source_order,
                                                                            json!({
                                                                                "status": "image-ready",
                                                                                "assetType": "video",
                                                                                "mediaId": media_id,
                                                                                "localPath": local_path,
                                                                                "remoteUrl": asset_url,
                                                                                "attemptState": "SUCCESSFUL",
                                                                                "remoteStatus": "LOCAL_READY"
                                                                            }),
                                                                        );
                                                                        use tauri::Emitter;
                                                                        let _ = app_for_event.emit("flowcontent-slot-updated", json!({
                                                                            "localProjectId": local_project_id_for_event,
                                                                            "sourceOrder": source_order,
                                                                            "status": "image-ready",
                                                                            "assetType": "video",
                                                                            "currentFileType": "image",
                                                                            "mediaId": media_id,
                                                                            "localPath": local_path,
                                                                            "remoteUrl": asset_url,
                                                                            "remoteStatus": "LOCAL_READY"
                                                                        }));
                                                                    }
                                                                    Err(error) => {
                                                                        eprintln!("[Bridge] Falha ao persistir imagem base do slot {}: {}", source_order, error);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else if msg_type == "FLOWCONTENT_COMMAND_RESULT" {
                                        let data = payload.get("payload").unwrap_or(&payload);
                                        let command_id = data.get("id").and_then(Value::as_str);
                                        let command_error =
                                            if data.get("ok").and_then(Value::as_bool)
                                                == Some(false)
                                            {
                                                data.get("error")
                                                    .and_then(Value::as_str)
                                                    .map(str::to_string)
                                            } else {
                                                None
                                            };
                                        if let Some(command_id) = command_id {
                                            if let Ok(mut pending) =
                                                bridge_clone.pending_commands.lock()
                                            {
                                                pending.remove(command_id);
                                            }
                                        }
                                        let app_handle_opt = bridge_clone
                                            .app_handle
                                            .lock()
                                            .ok()
                                            .and_then(|guard| guard.clone());
                                        let applied_locally = match app_handle_opt.as_ref() {
                                            Some(app) => {
                                                match apply_create_project_result(app, data) {
                                                    Ok(value) => value,
                                                    Err(error) => {
                                                        eprintln!("[Bridge] Falha ao aplicar retorno CREATE_PROJECT: {}", error);
                                                        false
                                                    }
                                                }
                                            }
                                            None => false,
                                        };
                                        if !applied_locally {
                                            if let Ok(mut results) =
                                                bridge_clone.command_results.lock()
                                            {
                                                results.push(data.clone());
                                            }
                                        }
                                        if let Ok(mut status) = bridge_clone.status.lock() {
                                            let pending_len = bridge_clone
                                                .pending_commands
                                                .lock()
                                                .ok()
                                                .map(|pending| pending.len())
                                                .unwrap_or(0);
                                            status.pending_command = if pending_len > 0 {
                                                Some(format!("{} comando(s) em voo", pending_len))
                                            } else {
                                                None
                                            };
                                            status.last_command_error = command_error;
                                        }
                                        let cmd_type =
                                            data.get("type").and_then(Value::as_str).unwrap_or("");
                                        let cmd_ok =
                                            data.get("ok").and_then(Value::as_bool) == Some(true);

                                        // After individual generation completes
                                        let is_gen_result = cmd_type == "GENERATE_IMAGE"
                                            || cmd_type == "GENERATE_VIDEO"
                                            || cmd_type == "GENERATE_VIDEO_FROM_IMAGE"
                                            || cmd_type == "ANIMATE_IMAGE";
                                        if is_gen_result {
                                            let app_handle_opt = bridge_clone
                                                .app_handle
                                                .lock()
                                                .ok()
                                                .and_then(|guard| guard.clone());
                                            if let Some(app) = app_handle_opt.as_ref() {
                                                use tauri::Emitter;
                                                let mut event_payload = data.clone();
                                                if let Some(object) = event_payload.as_object_mut()
                                                {
                                                    object.insert(
                                                        "eventStatus".to_string(),
                                                        json!(if cmd_ok {
                                                            "COMMAND_OK"
                                                        } else {
                                                            "COMMAND_FAILED"
                                                        }),
                                                    );
                                                    object.insert(
                                                        "commandType".to_string(),
                                                        json!(cmd_type),
                                                    );
                                                }
                                                let _ = app.emit(
                                                    "flowcontent-slot-updated",
                                                    event_payload,
                                                );
                                            }
                                            let source_order = data
                                                .get("sourceOrder")
                                                .and_then(Value::as_u64)
                                                .unwrap_or(0)
                                                as usize;
                                            let mut treat_as_intermediate_image = false;
                                            if cmd_ok {
                                                if let Ok(mut queue) =
                                                    bridge_clone.generation_queue.lock()
                                                {
                                                    if let Some(q) = queue.as_mut() {
                                                        if let Some(command_id) = command_id {
                                                            q.in_flight.remove(command_id);
                                                        }
                                                        let counts_as_completed = !(q.mode
                                                            == "IMAGE_TO_VIDEO"
                                                            && q.phase
                                                                == IMAGE_TO_VIDEO_PHASE_GENERATE
                                                            && cmd_type == "GENERATE_IMAGE");
                                                        treat_as_intermediate_image =
                                                            !counts_as_completed
                                                                && cmd_type == "GENERATE_IMAGE";
                                                        if counts_as_completed {
                                                            q.completed_assets.push(data.clone());
                                                        }
                                                        record_generation_command(
                                                            &q.project_root,
                                                            &q.local_project_id,
                                                            &q.flow_project_id,
                                                            data,
                                                            "completed",
                                                            None,
                                                        );
                                                        println!(
                                                            "[Queue] Slot {} concluido. {} prontos finais, {} em voo, {} no total.",
                                                            source_order,
                                                            q.completed_assets.len(),
                                                            q.in_flight.len(),
                                                            q.total_prompts
                                                        );
                                                        persist_generation_queue_state(q);
                                                    }
                                                }
                                                if let Some(app_handle) = app_handle_opt.clone() {
                                                    let registry = read_registry(&app_handle);
                                                    if let Ok(registry) = registry {
                                                        let local_project_id = data
                                                            .get("localProjectId")
                                                            .and_then(Value::as_str)
                                                            .unwrap_or("");
                                                        if let Some(entry) =
                                                            registry.projects.iter().find(|p| {
                                                                p.local_project_id
                                                                    == local_project_id
                                                            })
                                                        {
                                                            let project_root =
                                                                entry.project_root.clone();
                                                            let asset_url = data
                                                                .get("url")
                                                                .and_then(Value::as_str)
                                                                .unwrap_or("")
                                                                .to_string();
                                                            let asset_type = data
                                                                .get("assetType")
                                                                .and_then(Value::as_str)
                                                                .unwrap_or("");
                                                            let media_id = data
                                                                .get("mediaId")
                                                                .and_then(Value::as_str)
                                                                .map(str::to_string);
                                                            let image_media_id = data
                                                                .get("imageMediaId")
                                                                .and_then(Value::as_str)
                                                                .map(str::to_string);
                                                            let command_id_value = data
                                                                .get("id")
                                                                .cloned()
                                                                .unwrap_or(Value::Null);
                                                            let workflow_id_value = data
                                                                .get("workflowId")
                                                                .cloned()
                                                                .unwrap_or(Value::Null);
                                                            let batch_id_value = data
                                                                .get("batchId")
                                                                .cloned()
                                                                .unwrap_or(Value::Null);
                                                            let operation_id_value = data
                                                                .get("operationId")
                                                                .cloned()
                                                                .unwrap_or(Value::Null);
                                                            let thumbnail_url_value = data
                                                                .get("thumbnailUrl")
                                                                .cloned()
                                                                .unwrap_or(Value::Null);
                                                            let remaining_credits_value = data
                                                                .get("remainingCredits")
                                                                .cloned()
                                                                .unwrap_or(Value::Null);
                                                            if source_order > 0
                                                                && !asset_url.is_empty()
                                                            {
                                                                let (extension, slot_type) =
                                                                    match asset_type {
                                                                        "mp4" => ("mp4", "video"),
                                                                        _ => ("png", "image"),
                                                                    };
                                                                let resolved_status =
                                                                    if treat_as_intermediate_image {
                                                                        "image-ready"
                                                                    } else {
                                                                        "ready"
                                                                    };
                                                                let resolved_asset_type =
                                                                    if treat_as_intermediate_image {
                                                                        "video"
                                                                    } else {
                                                                        slot_type
                                                                    };
                                                                tokio::spawn(async move {
                                                                    match download_asset_to_project(
                                                                        project_root.clone(),
                                                                        source_order,
                                                                        asset_url.clone(),
                                                                        extension,
                                                                        slot_type,
                                                                    )
                                                                    .await
                                                                    {
                                                                        Ok(path) => {
                                                                            let _ = update_slot_attempt_fields(
                                                                                &project_root,
                                                                                source_order,
                                                                                json!({
                                                                                    "status": resolved_status,
                                                                                    "assetType": resolved_asset_type,
                                                                                    "currentFileType": slot_type,
                                                                                    "mediaId": media_id,
                                                                                    "imageMediaId": image_media_id,
                                                                                    "localPath": path.to_string_lossy().to_string(),
                                                                                    "remoteUrl": asset_url,
                                                                                    "error": Value::Null,
                                                                                    "commandId": command_id_value,
                                                                                    "workflowId": workflow_id_value,
                                                                                    "batchId": batch_id_value,
                                                                                    "operationId": operation_id_value,
                                                                                    "thumbnailUrl": thumbnail_url_value,
                                                                                    "remoteStatus": if treat_as_intermediate_image { "REMOTE_IMAGE_READY" } else { "LOCAL_READY" },
                                                                                    "remainingCredits": remaining_credits_value,
                                                                                    "attemptState": "SUCCESSFUL"
                                                                                }),
                                                                            );
                                                                        }
                                                                        Err(error) => {
                                                                            eprintln!("[Bridge] Falha ao persistir slot {}: {}", source_order, error);
                                                                            let _ = update_slot_attempt_fields(
                                                                                &project_root,
                                                                                source_order,
                                                                                json!({
                                                                                    "status": "failed",
                                                                                    "error": error,
                                                                                    "remoteStatus": "LOCAL_DOWNLOAD_FAILED",
                                                                                    "attemptState": "FAILED"
                                                                                }),
                                                                            );
                                                                        }
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                let error_msg = data
                                                    .get("error")
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("Erro desconhecido")
                                                    .to_string();
                                                if let Ok(mut queue) =
                                                    bridge_clone.generation_queue.lock()
                                                {
                                                    if let Some(q) = queue.as_mut() {
                                                        if let Some(command_id) = command_id {
                                                            q.in_flight.remove(command_id);
                                                        }
                                                        if source_order > 0 {
                                                            q.failed_slots.push((
                                                                source_order,
                                                                error_msg.clone(),
                                                            ));
                                                        }
                                                        record_generation_command(
                                                            &q.project_root,
                                                            &q.local_project_id,
                                                            &q.flow_project_id,
                                                            data,
                                                            "failed",
                                                            Some(&error_msg),
                                                        );
                                                        println!(
                                                            "[Queue] Slot {} falhou: {}. {} em voo, {} concluido(s).",
                                                            source_order,
                                                            error_msg,
                                                            q.in_flight.len(),
                                                            q.completed_assets.len()
                                                        );
                                                        persist_generation_queue_state(q);
                                                    }
                                                }
                                                if let Some(app_handle) = app_handle_opt {
                                                    if let Ok(registry) = read_registry(&app_handle)
                                                    {
                                                        let local_project_id = data
                                                            .get("localProjectId")
                                                            .and_then(Value::as_str)
                                                            .unwrap_or("");
                                                        if let Some(entry) =
                                                            registry.projects.iter().find(|p| {
                                                                p.local_project_id
                                                                    == local_project_id
                                                            })
                                                        {
                                                            let _ = update_slot_attempt_fields(
                                                                &entry.project_root,
                                                                source_order,
                                                                json!({
                                                                    "status": "failed",
                                                                    "error": error_msg,
                                                                    "remoteStatus": "REMOTE_FAILED",
                                                                    "attemptState": "FAILED",
                                                                    "commandId": data.get("id").cloned().unwrap_or(Value::Null),
                                                                    "workflowId": data.get("workflowId").cloned().unwrap_or(Value::Null),
                                                                    "batchId": data.get("batchId").cloned().unwrap_or(Value::Null),
                                                                    "operationId": data.get("operationId").cloned().unwrap_or(Value::Null)
                                                                }),
                                                            );
                                                        }
                                                    }
                                                }
                                            }

                                            pump_generation_queue(&bridge_clone);

                                            let mut should_continue_image_to_video = false;
                                            let final_result = bridge_clone
                                                .generation_queue
                                                .lock()
                                                .ok()
                                                .and_then(|mut queue| {
                                                    let q = queue.as_mut()?;
                                                    let done_dispatching =
                                                        q.next_index >= q.prompts.len();
                                                    let fully_settled =
                                                        done_dispatching && q.in_flight.is_empty();
                                                    if !fully_settled {
                                                        return None;
                                                    }
                                                    if q.mode == "IMAGE_TO_VIDEO" {
                                                        match advance_image_to_video_queue(q) {
                                                            Ok(true) => {}
                                                            Ok(false) => {
                                                                should_continue_image_to_video =
                                                                    true;
                                                                return None;
                                                            }
                                                            Err(error) => {
                                                                q.active = false;
                                                                q.paused = false;
                                                                q.failed_slots
                                                                    .push((source_order, error));
                                                                persist_generation_queue_state(q);
                                                                return None;
                                                            }
                                                        }
                                                    }
                                                    q.active = false;
                                                    persist_generation_queue_state(q);
                                                    Some(json!({
                                                        "type": "GENERATION_COMPLETE",
                                                        "ok": true,
                                                        "localProjectId": q.local_project_id,
                                                        "projectId": q.flow_project_id,
                                                        "mode": q.mode,
                                                        "assets": q.completed_assets
                                                    }))
                                                });
                                            if should_continue_image_to_video {
                                                pump_generation_queue(&bridge_clone);
                                            }
                                            if let Some(result) = final_result {
                                                println!("[Queue] Projeto concluido. Finalizando estado local.");
                                                let app_handle_opt = bridge_clone
                                                    .app_handle
                                                    .lock()
                                                    .ok()
                                                    .and_then(|guard| guard.clone());
                                                if let Some(app_handle) = app_handle_opt {
                                                    handle_completed_generation(
                                                        &app_handle,
                                                        &result,
                                                    );
                                                }
                                            }
                                        }
                                    } else if msg_type == "FLOWCONTENT_LOG" {
                                        if let Some(payload_data) = payload.get("payload") {
                                            let msg = payload_data
                                                .get("message")
                                                .and_then(Value::as_str)
                                                .unwrap_or("");
                                            println!("[Chrome-Bridge] {}", msg);
                                            let app_handle_opt = bridge_clone
                                                .app_handle
                                                .lock()
                                                .ok()
                                                .and_then(|guard| guard.clone());
                                            if let Some(app) = app_handle_opt {
                                                use tauri::Emitter;
                                                let _ = app.emit("flowcontent-bridge-log", msg);
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(Message::Close(_)) | Err(_) => break,
                            _ => {}
                        }
                    }

                    // Clean up connection
                    println!("[Bridge] Extensão desconectada");
                    if let Ok(mut sender_guard) = bridge_clone.ws_sender.lock() {
                        *sender_guard = None;
                    }
                    if let Ok(mut status) = bridge_clone.status.lock() {
                        status.extension_connected = false;
                    }
                    send_task.abort();
                });
            }
            Err(e) => {
                eprintln!("[Bridge] Erro ao aceitar conexão: {}", e);
            }
        }
    }
}

fn handle_completed_generation(app: &tauri::AppHandle, result: &Value) {
    let Some(local_id) = result
        .get("localProjectId")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };

    let app_clone = app.clone();
    tokio::spawn(async move {
        println!("[Bridge] Finalizando geracao da producao {}", local_id);

        let registry = match read_registry(&app_clone) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[Bridge] Falha ao ler registro ao finalizar geracao: {}", e);
                return;
            }
        };

        let Some(entry) = registry
            .projects
            .iter()
            .find(|p| p.local_project_id == local_id)
        else {
            eprintln!("[Bridge] Producao {} nao encontrada no registro.", local_id);
            return;
        };

        mark_project_media_ready(&entry.project_root);
    });
}

fn candidate_workspace_roots(app: &tauri::AppHandle, app_data_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(document_dir) = app.path().document_dir() {
        candidates.push(document_dir.join("FlowContent Auto"));
    }
    if let Ok(user_profile) = env::var("USERPROFILE") {
        candidates.push(
            PathBuf::from(&user_profile)
                .join("OneDrive")
                .join("Documents")
                .join("FlowContent Auto"),
        );
        candidates.push(
            PathBuf::from(user_profile)
                .join("Documents")
                .join("FlowContent Auto"),
        );
    }
    candidates.push(app_data_dir.join("FlowContent Auto"));

    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(path.to_string_lossy().to_ascii_lowercase()));
    candidates
}

fn workspace_root_usable(root: &Path) -> bool {
    if root.exists() {
        return true;
    }
    root.parent().is_some_and(Path::exists)
}

fn workspace_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("settings").join("workspace.json"))
        .map_err(|error| format!("Nao foi possivel localizar a configuracao do workspace: {error}"))
}

fn save_workspace_config(app: &tauri::AppHandle, workspace_root: &Path) -> Result<(), String> {
    write_json(
        &workspace_config_path(app)?,
        &WorkspaceConfig {
            version: WORKSPACE_CONFIG_VERSION,
            workspace_root: workspace_root.to_path_buf(),
        },
    )?;
    let _ = save_app_setting(
        "workspaceRoot",
        &json!(workspace_root.to_string_lossy().to_string()),
    );
    Ok(())
}

fn load_workspace_config(app: &tauri::AppHandle) -> Result<Option<WorkspaceConfig>, String> {
    let path = workspace_config_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let config: WorkspaceConfig = read_json_typed(&path)?;
    if config.version != WORKSPACE_CONFIG_VERSION {
        return Ok(None);
    }
    Ok(Some(config))
}

fn app_data_root_from_env() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|value| PathBuf::from(value).join("com.flowcontent.auto"))
}

fn central_db_path() -> Result<PathBuf, String> {
    app_data_root_from_env()
        .map(|root| root.join("flowcontent.db"))
        .ok_or_else(|| "Nao foi possivel localizar APPDATA para o banco central.".to_string())
}

fn open_central_db() -> Result<Connection, String> {
    let path = central_db_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("Nao foi possivel preparar a pasta do banco central: {error}")
        })?;
    }
    let conn = Connection::open(&path).map_err(|error| {
        format!(
            "Nao foi possivel abrir o banco central {}: {error}",
            path.display()
        )
    })?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS projects (
            local_project_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            flow_project_id TEXT,
            project_root TEXT NOT NULL,
            manifest_path TEXT NOT NULL,
            stage TEXT,
            asset_count INTEGER NOT NULL DEFAULT 0,
            prompt_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT,
            updated_at TEXT NOT NULL,
            last_opened_at TEXT,
            deleted_at TEXT
        );
        CREATE TABLE IF NOT EXISTS slots (
            local_project_id TEXT NOT NULL,
            source_order INTEGER NOT NULL,
            prompt TEXT NOT NULL DEFAULT '',
            status TEXT,
            asset_type TEXT,
            current_file_type TEXT,
            local_path TEXT,
            remote_url TEXT,
            media_id TEXT,
            image_media_id TEXT,
            error TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (local_project_id, source_order)
        );
        CREATE TABLE IF NOT EXISTS generation_commands (
            command_id TEXT PRIMARY KEY,
            local_project_id TEXT,
            flow_project_id TEXT,
            source_order INTEGER,
            command_type TEXT,
            prompt TEXT,
            status TEXT NOT NULL,
            error TEXT,
            asset_url TEXT,
            media_id TEXT,
            image_media_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS generation_queue_state (
            local_project_id TEXT PRIMARY KEY,
            active INTEGER NOT NULL,
            paused INTEGER NOT NULL,
            mode TEXT,
            next_index INTEGER NOT NULL,
            completed_prompts INTEGER NOT NULL,
            total_prompts INTEGER NOT NULL,
            remaining_prompts INTEGER NOT NULL,
            target_source_orders_json TEXT NOT NULL,
            in_flight_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        ",
    )
    .map_err(|error| format!("Nao foi possivel inicializar o schema do banco central: {error}"))?;
    ensure_column(
        &conn,
        "slots",
        "slot_id",
        "ALTER TABLE slots ADD COLUMN slot_id TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "active_attempt_id",
        "ALTER TABLE slots ADD COLUMN active_attempt_id TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "attempt_count",
        "ALTER TABLE slots ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &conn,
        "slots",
        "attempts_json",
        "ALTER TABLE slots ADD COLUMN attempts_json TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "command_id",
        "ALTER TABLE slots ADD COLUMN command_id TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "workflow_id",
        "ALTER TABLE slots ADD COLUMN workflow_id TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "batch_id",
        "ALTER TABLE slots ADD COLUMN batch_id TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "operation_id",
        "ALTER TABLE slots ADD COLUMN operation_id TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "thumbnail_url",
        "ALTER TABLE slots ADD COLUMN thumbnail_url TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "remote_status",
        "ALTER TABLE slots ADD COLUMN remote_status TEXT",
    )?;
    ensure_column(
        &conn,
        "slots",
        "remaining_credits",
        "ALTER TABLE slots ADD COLUMN remaining_credits INTEGER",
    )?;
    ensure_column(
        &conn,
        "slots",
        "remote_updated_at",
        "ALTER TABLE slots ADD COLUMN remote_updated_at TEXT",
    )?;
    Ok(conn)
}

fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<(), String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| {
            format!("Nao foi possivel inspecionar schema da tabela {table}: {error}")
        })?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("Nao foi possivel consultar colunas de {table}: {error}"))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("Nao foi possivel iterar colunas de {table}: {error}"))?
    {
        let current: String = row
            .get(1)
            .map_err(|error| format!("Nao foi possivel ler coluna de {table}: {error}"))?;
        if current == column {
            return Ok(());
        }
    }
    conn.execute(ddl, [])
        .map_err(|error| format!("Nao foi possivel migrar coluna {table}.{column}: {error}"))?;
    Ok(())
}

fn save_app_setting(key: &str, value: &Value) -> Result<(), String> {
    let conn = open_central_db()?;
    conn.execute(
        "
        INSERT INTO app_settings (key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        ",
        params![key, value.to_string(), now_string()],
    )
    .map_err(|error| format!("Nao foi possivel gravar a configuracao central {key}: {error}"))?;
    Ok(())
}

fn project_entry_from_registry_by_root(
    project_root: &Path,
) -> Result<Option<ProjectEntry>, String> {
    let workspace_root = if let Some(root) = project_root.parent() {
        root.to_path_buf()
    } else {
        return Ok(None);
    };
    let registry_path = workspace_root.join(".flowcontent").join("projects.json");
    if !registry_path.exists() {
        return Ok(None);
    }
    let registry: Registry = read_json_typed(&registry_path)?;
    let wanted = project_root.to_string_lossy().to_ascii_lowercase();
    Ok(registry.projects.into_iter().find(|entry| {
        entry.project_root.to_string_lossy().to_ascii_lowercase() == wanted
            || entry.project_root == project_root
    }))
}

fn sync_project_snapshot_to_central_db(project_root: &Path) -> Result<(), String> {
    let production_path = project_root.join(".flowcontent").join("production.json");
    if !production_path.exists() {
        return Ok(());
    }
    let production = read_json(&production_path)?;
    let prompts = project_root.join("prompts").join("ordered-prompts.json");
    let prompts_json = if prompts.exists() {
        read_json(&prompts)?
    } else {
        json!({})
    };
    let prompt_items = prompts_json
        .get("prompts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let registry_entry = project_entry_from_registry_by_root(project_root)?;
    let local_project_id = production
        .get("localProjectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            registry_entry
                .as_ref()
                .map(|entry| entry.local_project_id.clone())
        })
        .ok_or_else(|| {
            format!(
                "Projeto sem localProjectId em {}.",
                production_path.display()
            )
        })?;
    let title = production
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| registry_entry.as_ref().map(|entry| entry.title.clone()))
        .unwrap_or_else(|| {
            project_root
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Projeto")
                .to_string()
        });
    let flow_project_id = production
        .get("flowProjectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            registry_entry
                .as_ref()
                .and_then(|entry| entry.flow_project_id.clone())
        });
    let manifest_path = registry_entry
        .as_ref()
        .map(|entry| entry.manifest_path.clone())
        .unwrap_or_else(|| project_root.join(".flowcontent").join("project.json"));
    let created_at = production
        .get("createdAt")
        .and_then(Value::as_str)
        .map(str::to_string);
    let updated_at = production
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let stage = production
        .get("stage")
        .and_then(Value::as_str)
        .map(str::to_string);
    let asset_count = production
        .get("assetCount")
        .and_then(Value::as_u64)
        .unwrap_or(prompt_items.len() as u64) as i64;
    let prompt_count = prompt_items.len() as i64;
    let last_opened_at = registry_entry
        .as_ref()
        .map(|entry| entry.last_opened_at.clone())
        .unwrap_or_else(now_string);

    let conn = open_central_db()?;
    conn.execute(
        "
        INSERT INTO projects (
            local_project_id, title, flow_project_id, project_root, manifest_path, stage,
            asset_count, prompt_count, created_at, updated_at, last_opened_at, deleted_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL)
        ON CONFLICT(local_project_id) DO UPDATE SET
            title = excluded.title,
            flow_project_id = excluded.flow_project_id,
            project_root = excluded.project_root,
            manifest_path = excluded.manifest_path,
            stage = excluded.stage,
            asset_count = excluded.asset_count,
            prompt_count = excluded.prompt_count,
            created_at = COALESCE(projects.created_at, excluded.created_at),
            updated_at = excluded.updated_at,
            last_opened_at = excluded.last_opened_at,
            deleted_at = NULL
        ",
        params![
            local_project_id,
            title,
            flow_project_id,
            project_root.to_string_lossy().to_string(),
            manifest_path.to_string_lossy().to_string(),
            stage,
            asset_count,
            prompt_count,
            created_at,
            updated_at,
            last_opened_at
        ],
    )
    .map_err(|error| format!("Nao foi possivel sincronizar projeto no banco central: {error}"))?;

    for slot in &slots {
        let Some(source_order) = slot_source_order(slot) else {
            continue;
        };
        let prompt = slot
            .get("prompt")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                prompt_items
                    .iter()
                    .find(|item| slot_source_order(item) == Some(source_order))
                    .and_then(|item| item.get("prompt").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .unwrap_or_default();
        conn.execute(
            "
            INSERT INTO slots (
                local_project_id, source_order, slot_id, prompt, status, asset_type, current_file_type,
                local_path, remote_url, media_id, image_media_id, error, active_attempt_id, attempt_count,
                attempts_json, command_id, workflow_id, batch_id, operation_id, thumbnail_url, remote_status,
                remaining_credits, remote_updated_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
            ON CONFLICT(local_project_id, source_order) DO UPDATE SET
                slot_id = excluded.slot_id,
                prompt = excluded.prompt,
                status = excluded.status,
                asset_type = excluded.asset_type,
                current_file_type = excluded.current_file_type,
                local_path = excluded.local_path,
                remote_url = excluded.remote_url,
                media_id = excluded.media_id,
                image_media_id = excluded.image_media_id,
                error = excluded.error,
                active_attempt_id = excluded.active_attempt_id,
                attempt_count = excluded.attempt_count,
                attempts_json = excluded.attempts_json,
                command_id = excluded.command_id,
                workflow_id = excluded.workflow_id,
                batch_id = excluded.batch_id,
                operation_id = excluded.operation_id,
                thumbnail_url = excluded.thumbnail_url,
                remote_status = excluded.remote_status,
                remaining_credits = excluded.remaining_credits,
                remote_updated_at = excluded.remote_updated_at,
                updated_at = excluded.updated_at
            ",
            params![
                local_project_id,
                source_order as i64,
                slot.get("slotId").and_then(Value::as_str).unwrap_or(&slot_id_for_source_order(source_order)),
                prompt,
                slot.get("status").and_then(Value::as_str),
                slot.get("assetType").and_then(Value::as_str),
                slot.get("currentFileType").and_then(Value::as_str),
                slot.get("localPath").and_then(Value::as_str),
                slot.get("remoteUrl").and_then(Value::as_str),
                slot.get("mediaId").and_then(Value::as_str),
                slot.get("imageMediaId").and_then(Value::as_str),
                slot.get("error").and_then(Value::as_str),
                slot.get("activeAttemptId").and_then(Value::as_str),
                slot.get("attemptCount").and_then(Value::as_u64).unwrap_or(0) as i64,
                slot.get("attempts").cloned().unwrap_or_else(|| json!([])).to_string(),
                slot.get("commandId").and_then(Value::as_str),
                slot.get("workflowId").and_then(Value::as_str),
                slot.get("batchId").and_then(Value::as_str),
                slot.get("operationId").and_then(Value::as_str),
                slot.get("thumbnailUrl").and_then(Value::as_str),
                slot.get("remoteStatus").and_then(Value::as_str),
                slot.get("remainingCredits").and_then(Value::as_i64),
                slot.get("remoteUpdatedAt").and_then(Value::as_str),
                updated_at
            ],
        ).map_err(|error| format!("Nao foi possivel sincronizar slot {source_order} no banco central: {error}"))?;
    }

    if let Some(state) = production.get("generationState") {
        conn.execute(
            "
            INSERT INTO generation_queue_state (
                local_project_id, active, paused, mode, next_index, completed_prompts,
                total_prompts, remaining_prompts, target_source_orders_json, in_flight_json, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(local_project_id) DO UPDATE SET
                active = excluded.active,
                paused = excluded.paused,
                mode = excluded.mode,
                next_index = excluded.next_index,
                completed_prompts = excluded.completed_prompts,
                total_prompts = excluded.total_prompts,
                remaining_prompts = excluded.remaining_prompts,
                target_source_orders_json = excluded.target_source_orders_json,
                in_flight_json = excluded.in_flight_json,
                updated_at = excluded.updated_at
            ",
            params![
                local_project_id,
                state.get("active").and_then(Value::as_bool).unwrap_or(false) as i64,
                state.get("paused").and_then(Value::as_bool).unwrap_or(false) as i64,
                state.get("mode").and_then(Value::as_str),
                state.get("nextIndex").and_then(Value::as_u64).unwrap_or(0) as i64,
                state.get("completedPrompts").and_then(Value::as_u64).unwrap_or(0) as i64,
                production.get("generationTotalPrompts").and_then(Value::as_u64).unwrap_or(slots.len() as u64) as i64,
                state.get("remainingPrompts").and_then(Value::as_u64).unwrap_or(0) as i64,
                state.get("targetSourceOrders").cloned().unwrap_or_else(|| json!([])).to_string(),
                state.get("inFlight").cloned().unwrap_or_else(|| json!([])).to_string(),
                updated_at
            ],
        ).map_err(|error| format!("Nao foi possivel sincronizar estado da fila no banco central: {error}"))?;
    } else {
        let _ = conn.execute(
            "DELETE FROM generation_queue_state WHERE local_project_id = ?1",
            params![local_project_id],
        );
    }
    Ok(())
}

fn sync_registry_to_central_db(app: &tauri::AppHandle) -> Result<(), String> {
    let registry = read_registry(app)?;
    for entry in &registry.projects {
        let _ = sync_project_snapshot_to_central_db(&entry.project_root);
    }
    Ok(())
}

fn load_generation_slots_from_central_db(local_project_id: &str) -> Result<Vec<Value>, String> {
    let conn = open_central_db()?;
    let mut statement = conn
        .prepare(
            "
            SELECT
                source_order,
                slot_id,
                prompt,
                COALESCE(status, 'queued'),
                COALESCE(asset_type, 'image'),
                current_file_type,
                local_path,
                remote_url,
                media_id,
                image_media_id,
                error,
                active_attempt_id,
                COALESCE(attempt_count, 0),
                attempts_json,
                command_id,
                workflow_id,
                batch_id,
                operation_id,
                thumbnail_url,
                remote_status,
                remaining_credits,
                remote_updated_at
            FROM slots
            WHERE local_project_id = ?1
            ORDER BY source_order ASC
            ",
        )
        .map_err(|error| format!("Nao foi possivel consultar slots no banco central: {error}"))?;
    let rows = statement
        .query_map(params![local_project_id], |row| {
            let asset_type: String = row.get(4)?;
            let attempts_json: Option<String> = row.get(13)?;
            let attempts: Value = attempts_json
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .unwrap_or_else(|| json!([]));
            Ok(json!({
                "sourceOrder": row.get::<_, i64>(0)? as usize,
                "slotId": row.get::<_, Option<String>>(1)?.unwrap_or_else(|| slot_id_for_source_order(row.get::<_, i64>(0).unwrap_or(0) as usize)),
                "prompt": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "assetType": asset_type,
                "currentFileType": row.get::<_, Option<String>>(5)?,
                "localPath": row.get::<_, Option<String>>(6)?,
                "remoteUrl": row.get::<_, Option<String>>(7)?,
                "mediaId": row.get::<_, Option<String>>(8)?,
                "imageMediaId": row.get::<_, Option<String>>(9)?,
                "error": row.get::<_, Option<String>>(10)?,
                "activeAttemptId": row.get::<_, Option<String>>(11)?,
                "attemptCount": row.get::<_, i64>(12)?,
                "attempts": attempts,
                "commandId": row.get::<_, Option<String>>(14)?,
                "workflowId": row.get::<_, Option<String>>(15)?,
                "batchId": row.get::<_, Option<String>>(16)?,
                "operationId": row.get::<_, Option<String>>(17)?,
                "thumbnailUrl": row.get::<_, Option<String>>(18)?,
                "remoteStatus": row.get::<_, Option<String>>(19)?,
                "remainingCredits": row.get::<_, Option<i64>>(20)?,
                "remoteUpdatedAt": row.get::<_, Option<String>>(21)?,
            }))
        })
        .map_err(|error| format!("Nao foi possivel iterar slots do banco central: {error}"))?;
    let mut slots = Vec::new();
    for row in rows {
        slots.push(row.map_err(|error| format!("Slot invalido no banco central: {error}"))?);
    }
    Ok(slots)
}

fn merge_generation_slots_for_view(
    production_slots: &[Value],
    central_slots: &[Value],
    downloads: &[DownloadedAssetInfo],
    prompts: &[Value],
) -> Vec<Value> {
    let prompt_by_order: HashMap<usize, &Value> = prompts
        .iter()
        .enumerate()
        .map(|(index, prompt)| (prompt_source_order(prompt, index), prompt))
        .collect();
    let local_by_order: HashMap<usize, &Value> = production_slots
        .iter()
        .filter_map(|slot| slot_source_order(slot).map(|order| (order, slot)))
        .collect();
    let central_by_order: HashMap<usize, &Value> = central_slots
        .iter()
        .filter_map(|slot| slot_source_order(slot).map(|order| (order, slot)))
        .collect();
    let downloaded_by_order: HashMap<usize, &DownloadedAssetInfo> = downloads
        .iter()
        .map(|asset| (asset.source_order, asset))
        .collect();

    let mut all_orders = HashSet::new();
    all_orders.extend(prompt_by_order.keys().copied());
    all_orders.extend(local_by_order.keys().copied());
    all_orders.extend(central_by_order.keys().copied());
    all_orders.extend(downloaded_by_order.keys().copied());

    let mut ordered: Vec<usize> = all_orders.into_iter().filter(|order| *order > 0).collect();
    ordered.sort_unstable();

    ordered
        .into_iter()
        .map(|source_order| {
            let mut merged = local_by_order
                .get(&source_order)
                .cloned()
                .cloned()
                .or_else(|| central_by_order.get(&source_order).cloned().cloned())
                .unwrap_or_else(|| {
                    let prompt_text = prompt_by_order
                        .get(&source_order)
                        .and_then(|prompt| prompt.get("prompt").and_then(Value::as_str))
                        .unwrap_or("")
                        .to_string();
                    json!({
                        "slotId": slot_id_for_source_order(source_order),
                        "sourceOrder": source_order,
                        "prompt": prompt_text,
                        "status": "queued",
                        "assetType": downloaded_by_order
                            .get(&source_order)
                            .map(|asset| asset.file_type)
                            .unwrap_or("image"),
                        "activeAttemptId": Value::Null,
                        "attemptCount": 0,
                        "attempts": Vec::<Value>::new(),
                        "commandId": Value::Null,
                        "workflowId": Value::Null,
                        "batchId": Value::Null,
                        "operationId": Value::Null,
                        "thumbnailUrl": Value::Null,
                        "remoteStatus": "LOCAL_QUEUED",
                        "remoteUpdatedAt": Value::Null,
                        "remainingCredits": Value::Null,
                        "currentFileType": Value::Null,
                        "localPath": Value::Null,
                        "remoteUrl": Value::Null,
                        "mediaId": Value::Null,
                        "imageMediaId": Value::Null,
                        "error": Value::Null
                    })
                });

            if let Some(downloaded) = downloaded_by_order.get(&source_order) {
                let expected_type = slot_expected_asset_type(&merged);
                let is_final_asset = expected_type != "video" || downloaded.file_type == "video";
                if let Some(obj) = merged.as_object_mut() {
                    obj.insert(
                        "currentFileType".to_string(),
                        Value::String(downloaded.file_type.to_string()),
                    );
                    obj.insert(
                        "localPath".to_string(),
                        Value::String(downloaded.full_path.clone()),
                    );
                    obj.insert(
                        "status".to_string(),
                        Value::String(
                            if is_final_asset {
                                "ready"
                            } else {
                                "image-ready"
                            }
                            .to_string(),
                        ),
                    );
                    obj.insert(
                        "remoteStatus".to_string(),
                        Value::String(
                            if is_final_asset {
                                "LOCAL_READY"
                            } else {
                                "REMOTE_IMAGE_READY"
                            }
                            .to_string(),
                        ),
                    );
                    obj.insert("error".to_string(), Value::Null);
                    if obj.get("assetType").is_none() || obj.get("assetType") == Some(&Value::Null)
                    {
                        obj.insert(
                            "assetType".to_string(),
                            Value::String(downloaded.file_type.to_string()),
                        );
                    }
                    if obj
                        .get("prompt")
                        .and_then(Value::as_str)
                        .is_none_or(|prompt| prompt.is_empty())
                    {
                        if let Some(prompt_text) = prompt_by_order
                            .get(&source_order)
                            .and_then(|prompt| prompt.get("prompt").and_then(Value::as_str))
                        {
                            obj.insert(
                                "prompt".to_string(),
                                Value::String(prompt_text.to_string()),
                            );
                        }
                    }
                }
            }

            merged
        })
        .collect()
}

fn pick_workspace_root(app: &tauri::AppHandle, app_data_dir: &Path) -> PathBuf {
    if let Ok(Some(config)) = load_workspace_config(app) {
        if workspace_root_usable(&config.workspace_root) {
            return config.workspace_root;
        }
    }
    let candidates = candidate_workspace_roots(app, app_data_dir);
    let with_projects = candidates
        .iter()
        .filter(|root| workspace_root_usable(root))
        .find(|root| {
            let registry_path = root.join(".flowcontent").join("projects.json");
            if !registry_path.is_file() {
                return false;
            }
            fs::read_to_string(&registry_path)
                .ok()
                .and_then(|contents| serde_json::from_str::<Registry>(&contents).ok())
                .is_some_and(|registry| !registry.projects.is_empty())
        })
        .cloned();
    if let Some(root) = with_projects {
        return root;
    }

    let with_registry = candidates
        .iter()
        .filter(|root| workspace_root_usable(root))
        .find(|root| root.join(".flowcontent").join("projects.json").is_file())
        .cloned();
    if let Some(root) = with_registry {
        return root;
    }

    let with_flowcontent = candidates
        .iter()
        .filter(|root| workspace_root_usable(root))
        .find(|root| root.join(".flowcontent").is_dir())
        .cloned();
    if let Some(root) = with_flowcontent {
        return root;
    }

    let selected = candidates
        .into_iter()
        .filter(|root| workspace_root_usable(root))
        .next()
        .unwrap_or_else(|| app_data_dir.join("FlowContent Auto"));
    let _ = save_workspace_config(app, &selected);
    selected
}

fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}

fn bundled_resource_roots(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        push_unique_path(&mut roots, resource_dir.clone());
        push_unique_path(&mut roots, resource_dir.join("_up_"));
    }

    if let Ok(executable_path) = env::current_exe() {
        if let Some(executable_dir) = executable_path.parent() {
            push_unique_path(&mut roots, executable_dir.to_path_buf());
            push_unique_path(&mut roots, executable_dir.join("_up_"));
        }
    }

    if let Ok(app_data_dir) = app.path().app_data_dir() {
        push_unique_path(&mut roots, app_data_dir.clone());
        push_unique_path(&mut roots, app_data_dir.join("_up_"));
    }

    roots
}

fn bundled_or_dev_path(app: &tauri::AppHandle, relative: &str) -> Result<PathBuf, String> {
    let bundled_candidates = bundled_resource_roots(app)
        .into_iter()
        .map(|root| root.join(relative))
        .collect::<Vec<_>>();

    for candidate in &bundled_candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    if !cfg!(debug_assertions) {
        let checked = bundled_candidates
            .iter()
            .map(|candidate| candidate.display().to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!(
            "Recurso empacotado ausente: {} (checado em: {})",
            relative, checked
        ));
    }

    let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| format!("Nao foi possivel localizar o recurso local: {relative}"))?
        .to_path_buf();
    Ok(repository_root.join(relative))
}

fn bundled_update_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    bundled_resource_roots(app)
        .into_iter()
        .map(|root| root.join("resources").join("update-config.json"))
        .find(|candidate| candidate.is_file())
}

fn dev_update_config_path() -> Option<PathBuf> {
    Some(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("update-config.json"),
    )
}

fn app_data_update_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Nao foi possivel localizar os dados do aplicativo: {error}"))?
        .join("update-config.json"))
}

fn load_updater_config(app: &tauri::AppHandle) -> Result<UpdaterConfigFile, String> {
    let mut candidates = Vec::new();
    candidates.push(app_data_update_config_path(app)?);
    if let Some(path) = bundled_update_config_path(app) {
        candidates.push(path);
    }
    if let Some(path) = dev_update_config_path() {
        candidates.push(path);
    }

    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let parsed: UpdaterConfigFile = read_json_typed(&candidate)?;
        return Ok(parsed);
    }

    Ok(UpdaterConfigFile::default())
}

fn normalized_update_endpoints(config: &UpdaterConfigFile) -> Vec<String> {
    config
        .endpoints
        .iter()
        .map(|endpoint| endpoint.trim())
        .filter(|endpoint| {
            !endpoint.is_empty()
                && !endpoint.contains("SEU_USUARIO")
                && !endpoint.contains("SEU_REPO")
        })
        .map(str::to_string)
        .collect()
}

#[tauri::command]
fn get_update_status(app: tauri::AppHandle) -> Result<UpdateStatus, String> {
    let config = load_updater_config(&app)?;
    let endpoints = normalized_update_endpoints(&config);
    Ok(UpdateStatus {
        enabled: config.enabled,
        configured: config.enabled && !endpoints.is_empty(),
        current_version: app.package_info().version.to_string(),
        endpoints,
    })
}

#[tauri::command]
async fn check_for_update(
    app: tauri::AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> Result<Option<UpdateMetadata>, String> {
    let status = get_update_status(app.clone())?;
    if !status.configured {
        let mut guard = pending_update
            .0
            .lock()
            .map_err(|_| "Nao foi possivel acessar o estado do updater.".to_string())?;
        *guard = None;
        return Ok(None);
    }

    let endpoints = status
        .endpoints
        .iter()
        .map(|endpoint| {
            Url::parse(endpoint).map_err(|error| format!("Endpoint de update invalido: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let update = app
        .updater_builder()
        .endpoints(endpoints)
        .map_err(|error| format!("Nao foi possivel configurar o updater: {error}"))?
        .build()
        .map_err(|error| format!("Nao foi possivel inicializar o updater: {error}"))?
        .check()
        .await
        .map_err(|error| format!("Falha ao verificar atualizacoes: {error}"))?;

    let metadata = update.as_ref().map(|update| UpdateMetadata {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
    });

    let mut guard = pending_update
        .0
        .lock()
        .map_err(|_| "Nao foi possivel acessar o estado do updater.".to_string())?;
    *guard = update;
    Ok(metadata)
}

#[tauri::command]
async fn install_pending_update(
    app: tauri::AppHandle,
    pending_update: State<'_, PendingUpdate>,
) -> Result<bool, String> {
    let update = {
        let mut guard = pending_update
            .0
            .lock()
            .map_err(|_| "Nao foi possivel acessar o estado do updater.".to_string())?;
        guard.take()
    };

    let Some(update) = update else {
        return Err("Nenhuma atualizacao pendente foi encontrada.".to_string());
    };

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| format!("Falha ao instalar atualizacao: {error}"))?;

    app.restart();
}

fn runtime_info(app: &tauri::AppHandle) -> Result<RuntimeInfo, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Nao foi possivel localizar os dados do aplicativo: {error}"))?;
    let documents_dir = app.path().document_dir().ok();
    let workspace_root = pick_workspace_root(app, &app_data_dir);

    Ok(RuntimeInfo {
        app_data_dir,
        documents_dir,
        workspace_root,
    })
}

fn require_auth(auth: &State<AuthState>) -> Result<(), String> {
    let authenticated = auth
        .authenticated
        .lock()
        .map_err(|_| "Nao foi possivel verificar a sessao.".to_string())?;
    if !*authenticated {
        return Err("Sessao bloqueada. Informe o token de acesso.".to_string());
    }
    Ok(())
}

fn token_matches(token: &str) -> bool {
    // Dev bypass — only in debug builds
    #[cfg(debug_assertions)]
    if token.trim().to_uppercase() == "CF-DEV-TEST-2024" || token.trim().to_uppercase() == "DEV" {
        return true;
    }
    check_credential(token)
}

fn diagnostic_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("diagnostics").join("manual-test.ndjson"))
        .map_err(|error| format!("Nao foi possivel localizar a pasta de diagnosticos: {error}"))
}

fn assemblyai_key_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(runtime_info(app)?
        .app_data_dir
        .join("secrets")
        .join("assemblyai.keys"))
}

fn read_assemblyai_keys(app: &tauri::AppHandle) -> Result<Vec<String>, String> {
    let path = assemblyai_key_file(app)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    fs::read_to_string(&path)
        .map_err(|error| format!("Nao foi possivel ler a configuracao do AssemblyAI: {error}"))
        .map(|contents| {
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
}

fn assemblyai_status(app: &tauri::AppHandle) -> Result<AssemblyAiStatus, String> {
    let keys = read_assemblyai_keys(app)?;
    Ok(AssemblyAiStatus {
        configured: !keys.is_empty(),
        key_count: keys.len(),
        masked_keys: keys
            .iter()
            .map(|key| {
                format!(
                    "...{}",
                    key.chars()
                        .rev()
                        .take(4)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                )
            })
            .collect(),
    })
}

#[tauri::command]
fn record_diagnostic_event(
    app: tauri::AppHandle,
    diagnostics: State<DiagnosticState>,
    mut event: Value,
) -> Result<(), String> {
    let _guard = diagnostics
        .writer
        .lock()
        .map_err(|_| "Nao foi possivel bloquear o arquivo de diagnosticos.".to_string())?;
    let path = diagnostic_file(&app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Pasta de diagnosticos invalida.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Nao foi possivel criar a pasta de diagnosticos: {error}"))?;

    if path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > 10 * 1024 * 1024)
    {
        let previous = parent.join("manual-test.previous.ndjson");
        let _ = fs::remove_file(&previous);
        fs::rename(&path, previous)
            .map_err(|error| format!("Nao foi possivel rotacionar os diagnosticos: {error}"))?;
    }

    let server_recorded_at = now_string();
    if let Some(object) = event.as_object_mut() {
        object.insert("serverRecordedAt".to_string(), json!(server_recorded_at));
    } else {
        event = json!({
            "serverRecordedAt": server_recorded_at,
            "type": "invalid-diagnostic-event"
        });
    }

    let serialized = serde_json::to_string(&event)
        .map_err(|error| format!("Nao foi possivel serializar o diagnostico: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("Nao foi possivel abrir o arquivo de diagnosticos: {error}"))?;
    writeln!(file, "{serialized}")
        .map_err(|error| format!("Nao foi possivel gravar o diagnostico: {error}"))
}

#[tauri::command]
fn get_auth_status(auth: State<AuthState>) -> Result<bool, String> {
    // Try to auto-authenticate from saved license first
    if let Ok(mut guard) = auth.authenticated.lock() {
        if !*guard {
            if let Some(saved_key) = load_license() {
                if check_credential(&saved_key) {
                    *guard = true;
                }
            }
        }
        return Ok(*guard);
    }
    Err("Nao foi possivel verificar a sessao.".to_string())
}

#[tauri::command]
fn validate_license(key: String, auth: State<AuthState>) -> Result<LicenseResult, String> {
    let normalized = key.trim().to_uppercase();

    // Dev bypass — only in debug builds
    #[cfg(debug_assertions)]
    if normalized == "CF-DEV-TEST-2024" || normalized == "DEV" {
        let _ = save_license(&normalized);
        if let Ok(mut guard) = auth.authenticated.lock() {
            *guard = true;
        }
        return Ok(LicenseResult {
            valid: true,
            message: "Modo desenvolvimento ativo.".into(),
        });
    }

    if check_credential(&normalized) {
        let _ = save_license(&normalized);
        if let Ok(mut guard) = auth.authenticated.lock() {
            *guard = true;
        }
        Ok(LicenseResult {
            valid: true,
            message: "Licença válida! ✓".into(),
        })
    } else {
        Ok(LicenseResult {
            valid: false,
            message: "Chave de acesso inválida.".into(),
        })
    }
}

#[tauri::command]
fn get_saved_license() -> Result<Option<String>, String> {
    Ok(load_license())
}

#[tauri::command]
fn authenticate(token: String, auth: State<AuthState>) -> Result<bool, String> {
    if !token_matches(&token) {
        return Err("Token de acesso invalido.".to_string());
    }
    let _ = save_license(&token.trim().to_uppercase());
    let mut authenticated = auth
        .authenticated
        .lock()
        .map_err(|_| "Nao foi possivel liberar a sessao.".to_string())?;
    *authenticated = true;
    Ok(true)
}

#[tauri::command]
fn lock_app(auth: State<AuthState>) -> Result<bool, String> {
    let mut authenticated = auth
        .authenticated
        .lock()
        .map_err(|_| "Nao foi possivel bloquear a sessao.".to_string())?;
    *authenticated = false;
    Ok(true)
}

#[tauri::command]
fn get_assemblyai_status(
    app: tauri::AppHandle,
    auth: State<AuthState>,
) -> Result<AssemblyAiStatus, String> {
    require_auth(&auth)?;
    assemblyai_status(&app)
}

#[tauri::command]
fn save_assemblyai_keys(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    keys: String,
) -> Result<AssemblyAiStatus, String> {
    require_auth(&auth)?;
    let keys: Vec<String> = keys
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if keys.is_empty() {
        return Err("Informe pelo menos uma chave da AssemblyAI.".to_string());
    }
    if keys
        .iter()
        .any(|key| key.len() < 16 || key.chars().any(char::is_whitespace))
    {
        return Err("Uma ou mais chaves da AssemblyAI parecem invalidas.".to_string());
    }

    let path = assemblyai_key_file(&app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Pasta de configuracao do AssemblyAI invalida.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Nao foi possivel criar a pasta de configuracao: {error}"))?;
    fs::write(&path, format!("{}\n", keys.join("\n")))
        .map_err(|error| format!("Nao foi possivel salvar as chaves da AssemblyAI: {error}"))?;
    assemblyai_status(&app)
}

#[tauri::command]
fn clear_assemblyai_keys(
    app: tauri::AppHandle,
    auth: State<AuthState>,
) -> Result<AssemblyAiStatus, String> {
    require_auth(&auth)?;
    let path = assemblyai_key_file(&app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            format!("Nao foi possivel remover as chaves da AssemblyAI: {error}")
        })?;
    }
    assemblyai_status(&app)
}

fn registry_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(runtime_info(app)?
        .workspace_root
        .join(".flowcontent")
        .join("projects.json"))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Nao foi possivel criar a pasta {}: {error}",
                parent.display()
            )
        })?;
    }
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Nao foi possivel serializar {}: {error}", path.display()))?;
    let temp_path = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json"),
        Uuid::new_v4()
    ));
    fs::write(&temp_path, format!("{serialized}\n")).map_err(|error| {
        format!(
            "Nao foi possivel gravar arquivo temporario {}: {error}",
            temp_path.display()
        )
    })?;
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("Nao foi possivel substituir {}: {error}", path.display()))?;
    }
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "Nao foi possivel finalizar gravacao de {}: {error}",
            path.display()
        )
    })
}

fn read_json(path: &Path) -> Result<Value, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("Nao foi possivel ler {}: {error}", path.display()))?;
    let mut stream = serde_json::Deserializer::from_str(&contents).into_iter::<Value>();
    let value = match stream.next() {
        Some(Ok(value)) => value,
        Some(Err(error)) => {
            return Err(format!("JSON invalido em {}: {error}", path.display()));
        }
        None => {
            return Err(format!(
                "JSON invalido em {}: arquivo vazio",
                path.display()
            ));
        }
    };

    let trailing = &contents[stream.byte_offset()..];
    if trailing.trim().is_empty() {
        return Ok(value);
    }

    write_json(path, &value)?;
    Ok(value)
}

fn read_json_typed<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let value = read_json(path)?;
    serde_json::from_value(value)
        .map_err(|error| format!("JSON estruturado invalido em {}: {error}", path.display()))
}

fn generation_ledger_path(project_root: &Path) -> PathBuf {
    project_root
        .join(".flowcontent")
        .join("generation-ledger.json")
}

fn ensure_generation_ledger(
    project_root: &Path,
    local_project_id: Option<&str>,
    flow_project_id: Option<&str>,
) -> Result<Value, String> {
    let path = generation_ledger_path(project_root);
    let mut ledger = if path.exists() {
        read_json(&path)?
    } else {
        json!({})
    };
    let object = ledger
        .as_object_mut()
        .ok_or_else(|| "Ledger de geracao invalido.".to_string())?;
    object.insert("version".to_string(), json!(GENERATION_LEDGER_VERSION));
    if let Some(local_project_id) = local_project_id.filter(|value| !value.trim().is_empty()) {
        object.insert("localProjectId".to_string(), json!(local_project_id));
    }
    if let Some(flow_project_id) = flow_project_id.filter(|value| !value.trim().is_empty()) {
        object.insert("flowProjectId".to_string(), json!(flow_project_id));
    }
    object
        .entry("commands".to_string())
        .or_insert_with(|| json!({}));
    object
        .entry("queue".to_string())
        .or_insert_with(|| json!({}));
    object.insert("updatedAt".to_string(), json!(now_string()));
    Ok(ledger)
}

fn write_generation_ledger(
    project_root: &Path,
    local_project_id: Option<&str>,
    flow_project_id: Option<&str>,
    ledger: &Value,
) -> Result<(), String> {
    let mut normalized = ledger.clone();
    let object = normalized
        .as_object_mut()
        .ok_or_else(|| "Ledger de geracao invalido.".to_string())?;
    object.insert("version".to_string(), json!(GENERATION_LEDGER_VERSION));
    if let Some(local_project_id) = local_project_id.filter(|value| !value.trim().is_empty()) {
        object.insert("localProjectId".to_string(), json!(local_project_id));
    }
    if let Some(flow_project_id) = flow_project_id.filter(|value| !value.trim().is_empty()) {
        object.insert("flowProjectId".to_string(), json!(flow_project_id));
    }
    object.insert("updatedAt".to_string(), json!(now_string()));
    write_json(&generation_ledger_path(project_root), &normalized)
}

fn persist_queue_snapshot_to_ledger(queue: &GenerationQueueState) {
    let mut ledger = match ensure_generation_ledger(
        &queue.project_root,
        Some(&queue.local_project_id),
        Some(&queue.flow_project_id),
    ) {
        Ok(ledger) => ledger,
        Err(error) => {
            eprintln!(
                "[Ledger] Falha ao preparar ledger do projeto {}: {}",
                queue.local_project_id, error
            );
            return;
        }
    };
    let Some(object) = ledger.as_object_mut() else {
        return;
    };
    object.insert(
        "queue".to_string(),
        json!({
            "active": queue.active,
            "paused": queue.paused,
            "mode": queue.mode,
            "nextIndex": queue.next_index,
            "completedPrompts": queue.completed_assets.len(),
            "totalPrompts": queue.total_prompts,
            "remainingPrompts": queue.total_prompts.saturating_sub(queue.completed_assets.len()),
            "targetSourceOrders": queue.target_source_orders,
            "inFlight": queue.in_flight,
            "updatedAt": now_string()
        }),
    );
    if let Err(error) = write_generation_ledger(
        &queue.project_root,
        Some(&queue.local_project_id),
        Some(&queue.flow_project_id),
        &ledger,
    ) {
        eprintln!(
            "[Ledger] Falha ao persistir snapshot da fila do projeto {}: {}",
            queue.local_project_id, error
        );
    }
}

fn record_generation_command(
    project_root: &Path,
    local_project_id: &str,
    flow_project_id: &str,
    command_like: &Value,
    status: &str,
    error: Option<&str>,
) {
    let command_id = command_like
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(command_id) = command_id else {
        return;
    };
    let mut ledger =
        match ensure_generation_ledger(project_root, Some(local_project_id), Some(flow_project_id))
        {
            Ok(ledger) => ledger,
            Err(err) => {
                eprintln!(
                    "[Ledger] Falha ao ler ledger para comando {}: {}",
                    command_id, err
                );
                return;
            }
        };
    let Some(root) = ledger.as_object_mut() else {
        return;
    };
    let Some(commands) = root.get_mut("commands").and_then(Value::as_object_mut) else {
        return;
    };
    let entry = commands.entry(command_id.clone()).or_insert_with(|| {
        json!({
            "id": command_id,
            "createdAt": now_string()
        })
    });
    let Some(command) = entry.as_object_mut() else {
        return;
    };
    command.insert("status".to_string(), json!(status));
    command.insert("updatedAt".to_string(), json!(now_string()));
    if let Some(command_type) = command_like.get("type").and_then(Value::as_str) {
        command.insert("type".to_string(), json!(command_type));
    }
    if let Some(prompt) = command_like.get("prompt").and_then(Value::as_str) {
        command.insert("prompt".to_string(), json!(prompt));
    }
    if let Some(source_order) = command_like
        .get("sourceOrder")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .filter(|value| *value > 0)
    {
        command.insert("sourceOrder".to_string(), json!(source_order));
    }
    if let Some(media_id) = command_like.get("mediaId").and_then(Value::as_str) {
        command.insert("mediaId".to_string(), json!(media_id));
    }
    if let Some(workflow_id) = command_like.get("workflowId").and_then(Value::as_str) {
        command.insert("workflowId".to_string(), json!(workflow_id));
    }
    if let Some(batch_id) = command_like.get("batchId").and_then(Value::as_str) {
        command.insert("batchId".to_string(), json!(batch_id));
    }
    if let Some(operation_id) = command_like.get("operationId").and_then(Value::as_str) {
        command.insert("operationId".to_string(), json!(operation_id));
    }
    if let Some(image_media_id) = command_like.get("imageMediaId").and_then(Value::as_str) {
        command.insert("imageMediaId".to_string(), json!(image_media_id));
    }
    if let Some(thumbnail_url) = command_like.get("thumbnailUrl").and_then(Value::as_str) {
        command.insert("thumbnailUrl".to_string(), json!(thumbnail_url));
    }
    if let Some(remote_status) = command_like.get("remoteStatus").and_then(Value::as_str) {
        command.insert("remoteStatus".to_string(), json!(remote_status));
    }
    if let Some(remaining_credits) = command_like.get("remainingCredits").and_then(Value::as_i64) {
        command.insert("remainingCredits".to_string(), json!(remaining_credits));
    }
    if let Some(asset_url) = command_like.get("url").and_then(Value::as_str) {
        command.insert("assetUrl".to_string(), json!(asset_url));
    }
    if let Some(error) = error.filter(|value| !value.trim().is_empty()) {
        command.insert("error".to_string(), json!(error));
    } else {
        command.remove("error");
    }
    if let Err(err) = write_generation_ledger(
        project_root,
        Some(local_project_id),
        Some(flow_project_id),
        &ledger,
    ) {
        eprintln!("[Ledger] Falha ao gravar comando {}: {}", command_id, err);
    }
}

fn first_matching_file(directory: &Path, suffix: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

fn first_matching_extension(directory: &Path, extensions: &[&str]) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    extensions
                        .iter()
                        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
                })
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

fn read_registry(app: &tauri::AppHandle) -> Result<Registry, String> {
    let path = registry_path(app)?;
    if !path.exists() {
        return Ok(Registry {
            version: REGISTRY_VERSION,
            projects: vec![],
        });
    }
    read_json_typed(&path).map_err(|error| format!("Registro de projetos invalido: {error}"))
}

fn save_registry(app: &tauri::AppHandle, registry: &Registry) -> Result<(), String> {
    write_json(&registry_path(app)?, registry)
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('-');
            last_was_separator = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "producao".to_string()
    } else {
        slug
    }
}

fn available_project_root(workspace_root: &Path, title: &str) -> PathBuf {
    let base = slugify(title);
    let mut suffix = 1;
    loop {
        let name = if suffix == 1 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let candidate = workspace_root.join(name);
        if !candidate.exists() {
            return candidate;
        }
        suffix += 1;
    }
}

fn project_summary(entry: &ProjectEntry) -> Result<ProjectSummary, String> {
    let metadata_root = entry.project_root.join(".flowcontent");
    let production_path = metadata_root.join("production.json");
    let segments_path = metadata_root.join("audio-segments.json");
    let prompts_path = entry
        .project_root
        .join("prompts")
        .join("ordered-prompts.json");

    let read_summary_json = |path: &Path| -> Value {
        if !path.exists() {
            return json!({});
        }
        match read_json(path) {
            Ok(value) => value,
            Err(error) => {
                eprintln!(
                    "[Projects] Ignorando metadado invalido em {}: {}",
                    path.display(),
                    error
                );
                json!({})
            }
        }
    };

    let mut production = read_summary_json(&production_path);
    let segments = read_summary_json(&segments_path);
    let prompts = read_summary_json(&prompts_path);
    let downloads = scan_downloaded_assets(&entry.project_root);
    let mut changed =
        reconcile_production_with_downloads(&entry.project_root, &mut production, &downloads);
    let asset_output_dir = project_asset_output_dir(&entry.project_root);
    let srt_root = entry.project_root.join("srt");
    let audio_root = entry.project_root.join("audio");
    let caption_srt_path = production
        .get("captionSrtPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| first_matching_file(&srt_root, ".legendas.srt"));
    let asset_srt_path = production
        .get("assetSrtPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| first_matching_file(&srt_root, ".assets.srt"));
    let audio_path = production
        .get("audioPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            first_matching_extension(
                &audio_root,
                &["mp3", "wav", "mp4", "m4a", "aac", "flac", "ogg"],
            )
        });
    if let Some(production_obj) = production.as_object_mut() {
        if let Some(path) = &caption_srt_path {
            changed |= set_object_field(
                production_obj,
                "captionSrtPath",
                Value::String(path.to_string_lossy().to_string()),
            );
        }
        if let Some(path) = &asset_srt_path {
            changed |= set_object_field(
                production_obj,
                "assetSrtPath",
                Value::String(path.to_string_lossy().to_string()),
            );
        }
        if let Some(path) = &audio_path {
            changed |= set_object_field(
                production_obj,
                "audioPath",
                Value::String(path.to_string_lossy().to_string()),
            );
        }
    }
    if changed {
        write_json(&production_path, &production)?;
    }

    Ok(ProjectSummary {
        local_project_id: entry.local_project_id.clone(),
        title: entry.title.clone(),
        flow_project_id: entry.flow_project_id.clone(),
        project_root: entry.project_root.clone(),
        asset_output_dir,
        stage: production
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or("AWAITING_AUDIO")
            .to_string(),
        asset_count: segments
            .get("assets")
            .and_then(Value::as_array)
            .map_or_else(
                || {
                    production
                        .get("assetCount")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize
                },
                Vec::len,
            ),
        prompt_count: prompts
            .get("prompts")
            .and_then(Value::as_array)
            .map_or_else(
                || {
                    production
                        .get("promptCount")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize
                },
                Vec::len,
            ),
        caption_srt_path,
        asset_srt_path,
        audio_path,
        updated_at: production
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or(&entry.last_opened_at)
            .to_string(),
    })
}

fn validate_project_root(app: &tauri::AppHandle, project_root: &Path) -> Result<PathBuf, String> {
    let workspace_root = fs::canonicalize(runtime_info(app)?.workspace_root)
        .map_err(|error| format!("Nao foi possivel validar a base local: {error}"))?;
    let canonical_project = fs::canonicalize(project_root)
        .map_err(|error| format!("Nao foi possivel validar a pasta da producao: {error}"))?;
    if !canonical_project.starts_with(&workspace_root)
        || !canonical_project.join(".flowcontent").is_dir()
    {
        return Err(
            "A pasta informada nao e uma producao registrada no FlowContent Auto.".to_string(),
        );
    }
    Ok(canonical_project)
}

fn update_production(project_root: &Path, values: Value) -> Result<(), String> {
    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    let target = production
        .as_object_mut()
        .ok_or_else(|| "Arquivo de producao invalido.".to_string())?;
    let additions = values
        .as_object()
        .ok_or_else(|| "Atualizacao de producao invalida.".to_string())?;
    for (key, value) in additions {
        target.insert(key.clone(), value.clone());
    }
    target.insert("updatedAt".to_string(), Value::String(now_string()));
    write_json(&production_path, &production)?;
    let _ = sync_project_snapshot_to_central_db(project_root);
    Ok(())
}

#[tauri::command]
fn get_runtime_info(app: tauri::AppHandle, auth: State<AuthState>) -> Result<RuntimeInfo, String> {
    require_auth(&auth)?;
    runtime_info(&app)
}

#[tauri::command]
fn initialize_workspace(
    app: tauri::AppHandle,
    auth: State<AuthState>,
) -> Result<RuntimeInfo, String> {
    require_auth(&auth)?;
    initialize_workspace_inner(&app)
}

fn initialize_workspace_inner(app: &tauri::AppHandle) -> Result<RuntimeInfo, String> {
    let info = runtime_info(app)?;
    let _ = open_central_db()?;
    fs::create_dir_all(info.workspace_root.join(".flowcontent"))
        .map_err(|error| format!("Nao foi possivel criar a base local: {error}"))?;
    if !registry_path(app)?.exists() {
        save_registry(
            app,
            &Registry {
                version: REGISTRY_VERSION,
                projects: vec![],
            },
        )?;
    }
    let _ = sync_registry_to_central_db(app);
    Ok(info)
}

fn prepare_bridge_extension(app: &tauri::AppHandle, bridge_token: &str) -> Result<PathBuf, String> {
    let source_root = bundled_or_dev_path(app, "extension")
        .map_err(|error| format!("Nao foi possivel localizar a extensao: {error}"))?;
    let target_root = runtime_info(app)?
        .app_data_dir
        .join("flow-bridge-extension");
    let target_icons = target_root.join("icons");
    fs::create_dir_all(&target_icons)
        .map_err(|error| format!("Nao foi possivel preparar a extensao: {error}"))?;

    fs::copy(
        source_root.join("manifest.json"),
        target_root.join("manifest.json"),
    )
    .map_err(|error| format!("Nao foi possivel copiar manifest.json: {error}"))?;
    fs::copy(
        source_root.join("content.js"),
        target_root.join("content.js"),
    )
    .map_err(|error| format!("Nao foi possivel copiar content.js: {error}"))?;
    fs::copy(
        source_root.join("page_bridge.js"),
        target_root.join("page_bridge.js"),
    )
    .map_err(|error| format!("Nao foi possivel copiar page_bridge.js: {error}"))?;
    let background = fs::read_to_string(source_root.join("background.js"))
        .map_err(|error| format!("Nao foi possivel ler a ponte da extensao: {error}"))?
        .replace("__FLOWCONTENT_BRIDGE_TOKEN__", bridge_token);
    fs::write(target_root.join("background.js"), background)
        .map_err(|error| format!("Nao foi possivel configurar a ponte da extensao: {error}"))?;
    for filename in ["icon-32.png", "icon-128.png"] {
        fs::copy(
            source_root.join("icons").join(filename),
            target_icons.join(filename),
        )
        .map_err(|error| format!("Nao foi possivel copiar o icone da extensao: {error}"))?;
    }
    Ok(target_root)
}

fn chrome_path() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Ok(program_files) = env::var("PROGRAMFILES") {
        candidates.push(
            PathBuf::from(program_files)
                .join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe"),
        );
    }
    if let Ok(program_files_x86) = env::var("PROGRAMFILES(X86)") {
        candidates.push(
            PathBuf::from(program_files_x86)
                .join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe"),
        );
    }
    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe"),
        );
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "Google Chrome nao foi encontrado neste computador.".to_string())
}

fn extension_installed_in_profile(chrome_profile: &Path, extension_path: &Path) -> bool {
    let expected_path = extension_path.to_string_lossy();
    ["Preferences", "Secure Preferences"]
        .into_iter()
        .filter_map(|filename| {
            fs::read_to_string(chrome_profile.join("Default").join(filename)).ok()
        })
        .filter_map(|preferences| serde_json::from_str::<Value>(&preferences).ok())
        .any(|preferences| {
            preferences
                .pointer("/extensions/settings")
                .and_then(Value::as_object)
                .is_some_and(|settings| {
                    settings.values().any(|extension| {
                        extension
                            .get("path")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.eq_ignore_ascii_case(&expected_path))
                    })
                })
        })
}

fn current_bridge_status(bridge: &FlowBridgeState) -> Result<FlowBridgeStatus, String> {
    let mut status = bridge
        .status
        .lock()
        .map_err(|_| "Nao foi possivel consultar a ponte Flow.".to_string())?
        .clone();
    if status
        .last_heartbeat_ms
        .is_none_or(|heartbeat| now_millis().saturating_sub(heartbeat) > 10_000)
    {
        status.extension_connected = false;
        status.flow_page_detected = false;
    }
    Ok(status)
}

fn restore_generation_queue(
    app: &tauri::AppHandle,
    bridge: &FlowBridgeState,
) -> Result<(), String> {
    let already_active = bridge
        .generation_queue
        .lock()
        .map_err(|_| "Nao foi possivel acessar a fila de geracao.".to_string())?
        .as_ref()
        .is_some_and(|queue| queue.active);
    if already_active {
        return Ok(());
    }

    let registry = read_registry(app)?;
    for entry in registry.projects {
        let production_path = entry
            .project_root
            .join(".flowcontent")
            .join("production.json");
        if !production_path.exists() {
            continue;
        }
        let mut production = read_json(&production_path)?;
        if reconcile_production_with_downloads(
            &entry.project_root,
            &mut production,
            &scan_downloaded_assets(&entry.project_root),
        ) {
            write_json(&production_path, &production)?;
        }
        if production.get("stage").and_then(Value::as_str) != Some("GENERATING_ASSETS") {
            continue;
        }
        let generation_state = production
            .get("generationState")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if generation_state.get("active").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let mode = production
            .get("generationMode")
            .and_then(Value::as_str)
            .unwrap_or("IMAGE")
            .to_string();
        let prompts = read_json(
            &entry
                .project_root
                .join("prompts")
                .join("ordered-prompts.json"),
        )?
        .get("prompts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
        let prompts = normalize_prompt_entries(&prompts);
        let slots = production
            .get("generationSlots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let target_source_orders = parse_source_orders(generation_state.get("targetSourceOrders"));
        let queued_prompts_all: Vec<Value> = if mode == "ANIMATE_IMAGES" {
            build_animation_queue_items(
                &prompts,
                &slots,
                if target_source_orders.is_empty() {
                    None
                } else {
                    Some(target_source_orders.as_slice())
                },
            )
            .into_iter()
            .filter(|item| {
                let source_order = item.get("sourceOrder").and_then(Value::as_u64).unwrap_or(0);
                slots
                    .iter()
                    .find(|slot| {
                        slot.get("sourceOrder").and_then(Value::as_u64) == Some(source_order)
                    })
                    .is_some_and(slot_requires_generation)
            })
            .collect()
        } else {
            prompts
                .iter()
                .filter(|prompt| {
                    let source_order = prompt
                        .get("sourceOrder")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    slots
                        .iter()
                        .find(|slot| {
                            slot.get("sourceOrder").and_then(Value::as_u64) == Some(source_order)
                        })
                        .is_none_or(slot_requires_generation)
                })
                .cloned()
                .collect()
        };
        if queued_prompts_all.is_empty() {
            continue;
        }
        let phase = generation_state
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or(if mode == "IMAGE_TO_VIDEO" {
                IMAGE_TO_VIDEO_PHASE_GENERATE
            } else {
                ""
            })
            .to_string();
        let queued_source_orders = parse_source_orders(generation_state.get("queuedSourceOrders"));
        let restored_all_prompts = if mode == "IMAGE_TO_VIDEO" && !queued_source_orders.is_empty() {
            select_prompt_subset(&queued_prompts_all, &queued_source_orders)
        } else {
            queued_prompts_all.clone()
        };
        let mut current_batch_source_orders =
            parse_source_orders(generation_state.get("currentBatchSourceOrders"));
        if mode == "IMAGE_TO_VIDEO" && current_batch_source_orders.is_empty() {
            current_batch_source_orders = source_orders_from_prompts(&restored_all_prompts)
                .into_iter()
                .take(normalized_queue_concurrency(
                    generation_state
                        .get("maxConcurrent")
                        .and_then(Value::as_u64)
                        .unwrap_or(2) as usize,
                ))
                .collect();
        }
        let queued_prompts = if mode == "IMAGE_TO_VIDEO" {
            if phase == IMAGE_TO_VIDEO_PHASE_ANIMATE {
                build_animation_queue_items(
                    &prompts,
                    &slots,
                    Some(current_batch_source_orders.as_slice()),
                )
            } else {
                select_prompt_subset(&restored_all_prompts, &current_batch_source_orders)
            }
        } else {
            queued_prompts_all.clone()
        };
        if queued_prompts.is_empty() && mode != "IMAGE_TO_VIDEO" {
            continue;
        }
        let effective_target_orders = if target_source_orders.is_empty() {
            queued_prompts_all
                .iter()
                .enumerate()
                .map(|(index, prompt)| prompt_source_order(prompt, index))
                .collect::<Vec<usize>>()
        } else {
            target_source_orders.clone()
        };
        let completed_orders: HashSet<usize> = slots
            .iter()
            .filter(|slot| slot_has_completed_local_asset(slot))
            .filter_map(slot_source_order)
            .filter(|order| effective_target_orders.contains(order))
            .collect();

        let settings = production
            .get("generationSettings")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let queue = GenerationQueueState {
            active: true,
            paused: generation_state
                .get("paused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            local_project_id: entry.local_project_id.clone(),
            flow_project_id: entry.flow_project_id.clone().unwrap_or_default(),
            project_root: entry.project_root.clone(),
            mode: mode.clone(),
            image_model: settings
                .get("imageModel")
                .and_then(Value::as_str)
                .unwrap_or("GEM_PIX_2")
                .to_string(),
            video_model: settings
                .get("videoModel")
                .and_then(Value::as_str)
                .unwrap_or("veo_3_1_t2v_lite_low_priority")
                .to_string(),
            i2v_model: settings
                .get("i2vModel")
                .and_then(Value::as_str)
                .unwrap_or("veo_3_1_i2v_lite_low_priority")
                .to_string(),
            image_aspect_ratio: settings
                .get("imageAspectRatio")
                .and_then(Value::as_str)
                .unwrap_or("IMAGE_ASPECT_RATIO_LANDSCAPE")
                .to_string(),
            video_aspect_ratio: settings
                .get("videoAspectRatio")
                .and_then(Value::as_str)
                .unwrap_or("VIDEO_ASPECT_RATIO_LANDSCAPE")
                .to_string(),
            prompts: queued_prompts.clone(),
            all_prompts: if mode == "IMAGE_TO_VIDEO" {
                restored_all_prompts
            } else {
                queued_prompts.clone()
            },
            phase,
            next_index: 0,
            completed_assets: effective_target_orders
                .iter()
                .filter(|order| completed_orders.contains(order))
                .map(|order| json!({ "sourceOrder": order }))
                .collect(),
            failed_slots: generation_state
                .get("failedSlots")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            Some((
                                item.get("sourceOrder").and_then(Value::as_u64)? as usize,
                                item.get("error")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Erro desconhecido")
                                    .to_string(),
                            ))
                        })
                        .collect::<Vec<(usize, String)>>()
                })
                .unwrap_or_default(),
            total_prompts: effective_target_orders.len(),
            max_concurrent: normalized_queue_concurrency(
                generation_state
                    .get("maxConcurrent")
                    .and_then(Value::as_u64)
                    .unwrap_or(2) as usize,
            ),
            target_source_orders: effective_target_orders,
            current_batch_source_orders,
            in_flight: HashMap::new(),
        };

        println!(
            "[Queue] Estado de geracao encontrado no projeto {} com {} prompt(s) pendente(s). Retomada automatica desativada.",
            queue.local_project_id,
            queue.prompts.len()
        );
        {
            let mut generation_queue = bridge
                .generation_queue
                .lock()
                .map_err(|_| "Nao foi possivel restaurar a fila de geracao.".to_string())?;
            let mut restored = queue.clone();
            restored.active = false;
            restored.paused = true;
            restored.in_flight.clear();
            *generation_queue = Some(restored.clone());
            persist_generation_queue_state(&restored);
        }
        return Ok(());
    }

    Ok(())
}

#[tauri::command]
fn get_flow_bridge_status(
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
) -> Result<FlowBridgeStatus, String> {
    require_auth(&auth)?;
    current_bridge_status(&bridge)
}

fn apply_create_project_result(app: &tauri::AppHandle, result: &Value) -> Result<bool, String> {
    if result.get("type").and_then(Value::as_str) != Some("CREATE_PROJECT")
        || result.get("ok").and_then(Value::as_bool) != Some(true)
    {
        return Ok(false);
    }
    let Some(local_id) = result.get("localProjectId").and_then(Value::as_str) else {
        return Ok(false);
    };
    let Some(flow_id) = result.get("projectId").and_then(Value::as_str) else {
        return Ok(false);
    };

    let mut registry = read_registry(app)?;
    let (manifest_path, project_root) = {
        let Some(entry) = registry
            .projects
            .iter_mut()
            .find(|entry| entry.local_project_id == local_id)
        else {
            return Ok(false);
        };

        if entry.flow_project_id.as_deref() == Some(flow_id) {
            return Ok(false);
        }

        entry.flow_project_id = Some(flow_id.to_string());
        (entry.manifest_path.clone(), entry.project_root.clone())
    };
    let mut manifest = read_json(&manifest_path)?;
    if let Some(object) = manifest.as_object_mut() {
        object.insert("flowProjectId".to_string(), json!(flow_id));
        object.insert("updatedAt".to_string(), json!(now_string()));
    }
    write_json(&manifest_path, &manifest)?;
    save_registry(app, &registry)?;
    let _ = sync_project_snapshot_to_central_db(&project_root);
    {
        use tauri::Emitter;
        let _ = app.emit(
            "flowcontent-project-linked",
            json!({
                "localProjectId": local_id,
                "flowProjectId": flow_id,
            }),
        );
    }
    Ok(true)
}

#[tauri::command]
fn open_flow_browser(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    flow_project_id: Option<String>,
) -> Result<FlowBridgeStatus, String> {
    require_auth(&auth)?;
    let extension_path = prepare_bridge_extension(&app, &bridge.token)?;
    let chrome_profile = runtime_info(&app)?.app_data_dir.join("flow-chrome-profile");
    fs::create_dir_all(&chrome_profile).map_err(|error| {
        format!("Nao foi possivel preparar o perfil dedicado do Chrome: {error}")
    })?;
    let extension_installed = extension_installed_in_profile(&chrome_profile, &extension_path);
    let url = if extension_installed {
        flow_project_id
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                format!(
                    "https://labs.google/fx/pt/tools/flow/project/{}",
                    value.trim()
                )
            })
            .unwrap_or_else(|| "https://labs.google/fx/pt/tools/flow".to_string())
    } else {
        "chrome://extensions".to_string()
    };

    Command::new(chrome_path()?)
        .arg(format!("--user-data-dir={}", chrome_profile.display()))
        .arg(format!("--load-extension={}", extension_path.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--start-minimized")
        .args(cfg!(debug_assertions).then_some("--remote-debugging-port=9223"))
        .arg("--new-window")
        .arg(url)
        .spawn()
        .map_err(|error| format!("Nao foi possivel abrir o Chrome dedicado: {error}"))?;
    if !extension_installed {
        let _ = Command::new("explorer.exe").arg(&extension_path).spawn();
    }

    if let Ok(mut status) = bridge.status.lock() {
        status.browser_opened = true;
        status.extension_installed = extension_installed;
        status.chrome_profile = Some(chrome_profile);
        status.extension_path = Some(extension_path);
    }
    current_bridge_status(&bridge)
}

#[tauri::command]
fn list_projects(
    app: tauri::AppHandle,
    auth: State<AuthState>,
) -> Result<Vec<ProjectSummary>, String> {
    require_auth(&auth)?;
    initialize_workspace_inner(&app)?;
    let registry = read_registry(&app)?;
    let mut projects = Vec::new();
    for entry in &registry.projects {
        match project_summary(entry) {
            Ok(summary) => projects.push(summary),
            Err(error) => {
                eprintln!(
                    "[Projects] Falha ao resumir projeto {} ({}): {}",
                    entry.title,
                    entry.project_root.display(),
                    error
                );
            }
        }
    }
    projects.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(projects)
}

#[tauri::command]
fn get_project_detail(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    project_root: PathBuf,
) -> Result<Value, String> {
    require_auth(&auth)?;
    let project_root = validate_project_root(&app, &project_root)?;
    let read_optional = |path: PathBuf| -> Result<Value, String> {
        if path.exists() {
            read_json(&path)
        } else {
            Ok(json!({}))
        }
    };
    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut production = read_optional(production_path.clone())?;
    let segments = read_optional(
        project_root
            .join(".flowcontent")
            .join("audio-segments.json"),
    )?;
    let ordered_prompts = read_optional(project_root.join("prompts").join("ordered-prompts.json"))?;
    let downloads = scan_downloaded_assets(&project_root);
    if reconcile_production_with_downloads(&project_root, &mut production, &downloads) {
        write_json(&production_path, &production)?;
    }
    let local_project_id = production
        .get("localProjectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Ok(mut queue_guard) = bridge.generation_queue.lock() {
        if let Some(queue) = queue_guard.as_mut() {
            if queue.project_root == project_root {
                let _ = reconcile_live_generation_queue(queue);
            }
        }
    }

    let production_slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let central_slots = if !local_project_id.is_empty() {
        load_generation_slots_from_central_db(&local_project_id).unwrap_or_default()
    } else {
        Vec::new()
    };
    let merged_slots = merge_generation_slots_for_view(
        &production_slots,
        &central_slots,
        &downloads,
        ordered_prompts
            .get("prompts")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    );

    Ok(json!({
        "production": production,
        "assets": segments.get("assets").cloned().unwrap_or_else(|| json!([])),
        "captions": segments.get("captions").cloned().unwrap_or_else(|| json!([])),
        "settings": segments.get("settings").cloned().unwrap_or_else(|| json!({})),
        "prompts": ordered_prompts.get("prompts").cloned().unwrap_or_else(|| json!([])),
        "generationSlots": Value::Array(merged_slots),
        "downloadedAssets": downloaded_assets_payload(&downloads)
    }))
}

#[tauri::command]
fn create_project(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    title: String,
    flow_project_id: Option<String>,
    asset_output_dir: Option<String>,
) -> Result<ProjectSummary, String> {
    require_auth(&auth)?;
    let clean_title = title.trim();
    if clean_title.is_empty() {
        return Err("Informe um nome para a producao.".to_string());
    }
    let clean_flow_id = flow_project_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if clean_flow_id.as_ref().is_some_and(|value| value.len() < 20) {
        return Err("O ID do projeto Flow parece incompleto.".to_string());
    }

    let info = initialize_workspace_inner(&app)?;
    let mut registry = read_registry(&app)?;
    if clean_flow_id.as_ref().is_some_and(|flow_id| {
        registry
            .projects
            .iter()
            .any(|project| project.flow_project_id.as_ref() == Some(flow_id))
    }) {
        return Err("Este projeto Flow ja esta vinculado a outra producao.".to_string());
    }

    let timestamp = now_string();
    let local_project_id = Uuid::new_v4().to_string();
    let project_root = available_project_root(&info.workspace_root, clean_title);
    let metadata_root = project_root.join(".flowcontent");
    let selected_asset_output_dir = asset_output_dir
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root.join("downloads"));
    for folder in ["audio", "srt", "prompts"] {
        fs::create_dir_all(project_root.join(folder))
            .map_err(|error| format!("Nao foi possivel criar a pasta da producao: {error}"))?;
    }
    fs::create_dir_all(&selected_asset_output_dir)
        .map_err(|error| format!("Nao foi possivel criar a pasta final dos assets: {error}"))?;
    fs::create_dir_all(&metadata_root)
        .map_err(|error| format!("Nao foi possivel criar os metadados da producao: {error}"))?;

    let manifest_path = metadata_root.join("project.json");
    write_json(
        &manifest_path,
        &json!({
            "version": 1,
            "localProjectId": local_project_id,
            "title": clean_title,
            "flowProjectId": clean_flow_id,
            "createdAt": timestamp,
            "updatedAt": timestamp,
            "assetOutputDir": selected_asset_output_dir,
            "remoteMediaStoredLocally": false
        }),
    )?;
    write_json(
        &metadata_root.join("production.json"),
        &json!({
            "version": 1,
            "localProjectId": local_project_id,
            "title": clean_title,
            "stage": "AWAITING_AUDIO",
            "assetOutputDir": selected_asset_output_dir,
            "createdAt": timestamp,
            "updatedAt": timestamp
        }),
    )?;
    write_json(
        &metadata_root.join("generation-ledger.json"),
        &json!({
            "version": GENERATION_LEDGER_VERSION,
            "localProjectId": local_project_id,
            "flowProjectId": clean_flow_id,
            "commands": {},
            "queue": {
                "active": false,
                "paused": false,
                "mode": Value::Null,
                "nextIndex": 0,
                "completedPrompts": 0,
                "totalPrompts": 0,
                "remainingPrompts": 0,
                "targetSourceOrders": Vec::<usize>::new(),
                "inFlight": {},
                "updatedAt": timestamp
            },
            "updatedAt": timestamp
        }),
    )?;

    let should_create_flow_project = clean_flow_id.is_none();
    let entry = ProjectEntry {
        local_project_id,
        title: clean_title.to_string(),
        flow_project_id: clean_flow_id,
        project_root,
        manifest_path,
        last_opened_at: timestamp,
    };
    let summary = project_summary(&entry)?;
    let local_project_id_for_command = entry.local_project_id.clone();
    let title_for_command = entry.title.clone();
    registry.projects.push(entry);
    save_registry(&app, &registry)?;
    let _ = sync_project_snapshot_to_central_db(&summary.project_root);
    if should_create_flow_project {
        let _ = queue_bridge_command(
            &bridge,
            json!({
                "id": Uuid::new_v4().to_string(),
                "type": "CREATE_PROJECT",
                "localProjectId": local_project_id_for_command,
                "title": title_for_command
            }),
        );
    }
    Ok(summary)
}

#[tauri::command]
fn delete_project(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    local_project_id: String,
) -> Result<bool, String> {
    require_auth(&auth)?;
    let mut registry = read_registry(&app)?;
    let Some(project_index) = registry
        .projects
        .iter()
        .position(|entry| entry.local_project_id == local_project_id)
    else {
        return Ok(false);
    };
    let project_root = validate_project_root(&app, &registry.projects[project_index].project_root)?;

    let removed_pending = if let Ok(mut pending) = bridge.pending_commands.lock() {
        let before = pending.len();
        pending.retain(|_, command| {
            command.get("localProjectId").and_then(Value::as_str) != Some(local_project_id.as_str())
        });
        before.saturating_sub(pending.len())
    } else {
        0
    };
    if removed_pending > 0 {
        if let Ok(mut status) = bridge.status.lock() {
            let pending_len = bridge
                .pending_commands
                .lock()
                .ok()
                .map(|pending| pending.len())
                .unwrap_or(0);
            status.pending_command = if pending_len > 0 {
                Some(format!("{} comando(s) em voo", pending_len))
            } else {
                None
            };
        }
    }
    if let Ok(mut results) = bridge.command_results.lock() {
        results.retain(|result| {
            result.get("localProjectId").and_then(Value::as_str) != Some(local_project_id.as_str())
        });
    }
    if let Ok(mut queue) = bridge.generation_queue.lock() {
        if queue
            .as_ref()
            .is_some_and(|state| state.local_project_id == local_project_id)
        {
            *queue = None;
        }
    }

    fs::remove_dir_all(&project_root)
        .map_err(|error| format!("Nao foi possivel excluir a pasta da producao: {error}"))?;
    if let Ok(conn) = open_central_db() {
        let _ = conn.execute(
            "UPDATE projects SET deleted_at = ?2, updated_at = ?2 WHERE local_project_id = ?1",
            params![local_project_id, now_string()],
        );
        let _ = conn.execute(
            "DELETE FROM slots WHERE local_project_id = ?1",
            params![local_project_id],
        );
        let _ = conn.execute(
            "DELETE FROM generation_queue_state WHERE local_project_id = ?1",
            params![local_project_id],
        );
    }
    registry.projects.remove(project_index);
    save_registry(&app, &registry)?;
    Ok(true)
}

#[tauri::command]
fn export_project_srt(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    project_root: PathBuf,
    kind: String,
    target_path: PathBuf,
) -> Result<bool, String> {
    require_auth(&auth)?;
    let project_root = validate_project_root(&app, &project_root)?;
    let production = read_json(&project_root.join(".flowcontent").join("production.json"))?;
    let key = match kind.as_str() {
        "captions" => "captionSrtPath",
        "assets" => "assetSrtPath",
        _ => return Err("Tipo de SRT invalido.".to_string()),
    };
    let source = production
        .get(key)
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "Este SRT ainda nao foi gerado.".to_string())?;
    let source = match fs::canonicalize(&source) {
        Ok(path) => path,
        Err(_) => {
            let suffix = match kind.as_str() {
                "captions" => ".legendas.srt",
                "assets" => ".assets.srt",
                _ => unreachable!(),
            };
            let fallback =
                first_matching_file(&project_root.join("srt"), suffix).ok_or_else(|| {
                    "Nao foi possivel validar o SRT: arquivo nao encontrado.".to_string()
                })?;
            fs::canonicalize(&fallback)
                .map_err(|error| format!("Nao foi possivel validar o SRT: {error}"))?
        }
    };
    if !source.starts_with(&project_root)
        || !source.is_file()
        || source.extension().and_then(|value| value.to_str()) != Some("srt")
    {
        return Err("Arquivo SRT de origem invalido.".to_string());
    }
    fs::copy(&source, &target_path)
        .map_err(|error| format!("Nao foi possivel salvar o SRT: {error}"))?;
    Ok(true)
}

#[tauri::command]
async fn export_capcut_project(
    app: tauri::AppHandle,
    auth: State<'_, AuthState>,
    project_root: PathBuf,
) -> Result<Value, String> {
    require_auth(&auth)?;
    let project_root = validate_project_root(&app, &project_root)?;
    let script_path = bundled_or_dev_path(&app, "capcut/export_draft.py")
        .map_err(|error| format!("Nao foi possivel localizar o exportador do CapCut: {error}"))?;
    let project_root_for_cmd = project_root.clone();
    let output = tauri::async_runtime::spawn_blocking(move || {
        let mut failures = Vec::new();
        for candidate in python_command_candidates() {
            let mut command = Command::new(&candidate.program);
            command.args(&candidate.prefix_args);
            command
                .arg(&script_path)
                .arg("--project-root")
                .arg(&project_root_for_cmd);
            match command.output() {
                Ok(output) => return Ok(output),
                Err(error) => {
                    failures.push(format!("{} ({error})", candidate.program.to_string_lossy()))
                }
            }
        }
        Err(format!(
            "Nao foi possivel localizar um interpretador Python executavel. Tentativas: {}",
            failures.join(", ")
        ))
    })
    .await
    .map_err(|error| format!("Falha ao aguardar o exportador do CapCut: {error}"))??;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(message.trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            return Err(
                "O exportador do CapCut terminou sem retornar dados. Verifique se o CapCut possui ao menos um draft local valido."
                    .to_string(),
            );
        }
        return Err(stderr.trim().to_string());
    }
    let result: Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("Resposta invalida do exportador do CapCut: {error}"))?;
    update_production(
        &project_root,
        json!({
            "capcutDraft": {
                "draftId": result.get("draftId").cloned().unwrap_or(Value::Null),
                "draftName": result.get("draftName").cloned().unwrap_or(Value::Null),
                "draftPath": result.get("draftPath").cloned().unwrap_or(Value::Null),
                "capcutRoot": result.get("capcutRoot").cloned().unwrap_or(Value::Null)
            }
        }),
    )?;
    Ok(result)
}

#[tauri::command]
fn sync_flow_project_links(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
) -> Result<usize, String> {
    require_auth(&auth)?;
    let mut results = bridge
        .command_results
        .lock()
        .map_err(|_| "Nao foi possivel ler os retornos da ponte Flow.".to_string())?;
    let completed = std::mem::take(&mut *results);
    drop(results);
    let mut changed = 0;
    for result in completed {
        if apply_create_project_result(&app, &result)? {
            changed += 1;
        }
    }
    Ok(changed)
}

#[tauri::command]
fn ensure_flow_project_link(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    local_project_id: String,
) -> Result<Option<String>, String> {
    require_auth(&auth)?;
    let registry = read_registry(&app)?;
    let entry = registry
        .projects
        .iter()
        .find(|entry| entry.local_project_id == local_project_id)
        .ok_or_else(|| "Producao nao registrada.".to_string())?;
    if entry.flow_project_id.is_some() {
        return Ok(None);
    }
    if let Ok(pending) = bridge.pending_commands.lock() {
        let already_pending = pending.values().any(|command| {
            command.get("type").and_then(Value::as_str) == Some("CREATE_PROJECT")
                && command.get("localProjectId").and_then(Value::as_str)
                    == Some(local_project_id.as_str())
        });
        if already_pending {
            return Ok(None);
        }
    }
    queue_bridge_command(
        &bridge,
        json!({
            "id": Uuid::new_v4().to_string(),
            "type": "CREATE_PROJECT",
            "localProjectId": entry.local_project_id,
            "title": entry.title
        }),
    )
    .map(Some)
}

#[tauri::command]
fn queue_project_generation(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    local_project_id: String,
    mode: String,
    image_model: Option<String>,
    video_model: Option<String>,
    i2v_model: Option<String>,
    image_aspect_ratio: Option<String>,
    video_aspect_ratio: Option<String>,
    max_concurrent: Option<usize>,
    source_orders: Option<Vec<usize>>,
    generation_strategy: Option<String>,
) -> Result<String, String> {
    require_auth(&auth)?;
    if !["IMAGE", "VIDEO", "IMAGE_TO_VIDEO"].contains(&mode.as_str()) {
        return Err("Modo de geracao invalido.".to_string());
    }
    let generation_strategy = generation_strategy.unwrap_or_else(|| "continue".to_string());
    if !["continue", "restart"].contains(&generation_strategy.as_str()) {
        return Err("Estrategia de geracao invalida.".to_string());
    }
    let registry = read_registry(&app)?;
    let entry = registry
        .projects
        .iter()
        .find(|entry| entry.local_project_id == local_project_id)
        .ok_or_else(|| "Producao nao registrada.".to_string())?;
    let project_root = validate_project_root(&app, &entry.project_root)?;
    if let Ok(queue) = bridge.generation_queue.lock() {
        if let Some(active_queue) = queue.as_ref() {
            if active_queue.active && active_queue.local_project_id != local_project_id {
                return Err(format!(
                    "Ja existe uma geracao ativa no projeto {}. Pause ou conclua essa fila antes de iniciar outra.",
                    active_queue.local_project_id
                ));
            }
        }
    }
    let flow_project_id = entry
        .flow_project_id
        .clone()
        .ok_or_else(|| "Aguarde o vinculo automatico com o projeto Flow.".to_string())?;
    let prompts = read_json(&project_root.join("prompts").join("ordered-prompts.json"))?
        .get("prompts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let prompts = normalize_prompt_entries(&prompts);
    if prompts.is_empty() {
        return Err("Salve os prompts antes de iniciar a geracao.".to_string());
    }

    let img_model = image_model.unwrap_or_else(|| "GEM_PIX_2".to_string());
    let vid_model = video_model.unwrap_or_else(|| "veo_3_1_t2v_lite_low_priority".to_string());
    let i2v_mod = i2v_model.unwrap_or_else(|| "veo_3_1_i2v_lite_low_priority".to_string());
    let img_ratio =
        image_aspect_ratio.unwrap_or_else(|| "IMAGE_ASPECT_RATIO_LANDSCAPE".to_string());
    let vid_ratio =
        video_aspect_ratio.unwrap_or_else(|| "VIDEO_ASPECT_RATIO_LANDSCAPE".to_string());
    let queue_concurrency = normalized_queue_concurrency(max_concurrent.unwrap_or(2));

    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut existing_production = if production_path.exists() {
        read_json(&production_path)?
    } else {
        json!({})
    };
    if reconcile_production_with_downloads(
        &project_root,
        &mut existing_production,
        &scan_downloaded_assets(&project_root),
    ) {
        write_json(&production_path, &existing_production)?;
    }
    let existing_slots = existing_production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing_target_orders = parse_source_orders(
        existing_production
            .get("generationState")
            .and_then(|value| value.get("targetSourceOrders")),
    );
    let can_continue = existing_slots.len() == prompts.len()
        && existing_slots.iter().any(slot_requires_generation);
    let selected_orders: Vec<usize> = source_orders
        .unwrap_or_default()
        .into_iter()
        .filter(|value| *value > 0)
        .collect();
    let filter_selected =
        |source_order: usize| selected_orders.is_empty() || selected_orders.contains(&source_order);
    let target_source_orders: Vec<usize> = if !selected_orders.is_empty() {
        selected_orders.clone()
    } else if generation_strategy == "continue"
        && can_continue
        && !existing_target_orders.is_empty()
    {
        existing_target_orders
    } else {
        prompts
            .iter()
            .enumerate()
            .map(|(index, prompt)| prompt_source_order(prompt, index))
            .collect()
    };
    let queued_prompts: Vec<Value> = if generation_strategy == "continue" && can_continue {
        prompts
            .iter()
            .enumerate()
            .filter(|(index, prompt)| {
                let source_order = prompt_source_order(prompt, *index);
                if !filter_selected(source_order) {
                    return false;
                }
                existing_slots
                    .iter()
                    .find(|slot| {
                        slot.get("sourceOrder").and_then(Value::as_u64) == Some(source_order as u64)
                    })
                    .is_none_or(slot_requires_generation)
            })
            .map(|(_, prompt)| prompt.clone())
            .collect()
    } else {
        prompts
            .iter()
            .enumerate()
            .filter(|(index, prompt)| {
                let source_order = prompt_source_order(prompt, *index);
                filter_selected(source_order)
            })
            .map(|(_, prompt)| prompt.clone())
            .collect()
    };
    let target_orders_set: HashSet<usize> = target_source_orders.iter().copied().collect();
    if generation_strategy == "restart" {
        clear_downloaded_assets_for_orders(&project_root, &target_orders_set)?;
    }
    let generation_slots = if generation_strategy == "restart" {
        reset_generation_slots_for_orders(&prompts, &mode, &existing_slots, &target_orders_set)
    } else if can_continue {
        existing_slots.clone()
    } else {
        build_generation_slots(&prompts, &mode)
    };
    let completed_orders: HashSet<usize> = existing_slots
        .iter()
        .filter(|slot| slot_has_completed_local_asset(slot))
        .filter_map(slot_source_order)
        .filter(|order| target_source_orders.contains(order))
        .collect();
    let completed_orders_count = if generation_strategy == "restart" {
        0
    } else {
        completed_orders.len()
    };
    let total_prompts = target_source_orders.len();
    if queued_prompts.is_empty() && completed_orders.len() >= total_prompts && total_prompts > 0 {
        return Ok("Todos os slots deste projeto ja foram gerados.".to_string());
    }
    if total_prompts == 0 {
        return Err("Nenhum slot valido foi encontrado para gerar.".to_string());
    }

    // Store queue state — flat list of prompts, processed one by one
    {
        let mut queue = bridge
            .generation_queue
            .lock()
            .map_err(|_| "Nao foi possivel acessar a fila de geracao.".to_string())?;
        let mut queue_state = GenerationQueueState {
            active: true,
            paused: false,
            local_project_id: local_project_id.clone(),
            flow_project_id: flow_project_id.clone(),
            project_root: project_root.clone(),
            mode: mode.clone(),
            image_model: img_model.clone(),
            video_model: vid_model.clone(),
            i2v_model: i2v_mod.clone(),
            image_aspect_ratio: img_ratio.clone(),
            video_aspect_ratio: vid_ratio.clone(),
            prompts: queued_prompts.clone(),
            all_prompts: queued_prompts.clone(),
            phase: if mode == "IMAGE_TO_VIDEO" {
                IMAGE_TO_VIDEO_PHASE_GENERATE.to_string()
            } else {
                String::new()
            },
            next_index: 0,
            completed_assets: if generation_strategy == "restart" {
                vec![]
            } else {
                target_source_orders
                    .iter()
                    .filter(|order| completed_orders.contains(order))
                    .map(|order| json!({ "sourceOrder": order }))
                    .collect()
            },
            failed_slots: vec![],
            total_prompts,
            max_concurrent: queue_concurrency,
            target_source_orders: target_source_orders.clone(),
            current_batch_source_orders: vec![],
            in_flight: HashMap::new(),
        };
        if mode == "IMAGE_TO_VIDEO" {
            initialize_image_to_video_batch(&mut queue_state);
        }
        *queue = Some(queue_state);
    }

    // Update production stage
    update_production(
        &project_root,
        json!({
            "stage": "GENERATING_ASSETS",
            "generationTotalPrompts": total_prompts,
            "generationMode": mode,
            "generationSettings": {
                "imageModel": img_model,
                "videoModel": vid_model,
                "i2vModel": i2v_mod,
                "imageAspectRatio": img_ratio,
                "videoAspectRatio": vid_ratio
            },
            "generationSlots": Value::Array(generation_slots),
            "generationState": {
                "active": true,
                "paused": false,
                "mode": mode,
                "nextIndex": 0,
                "completedPrompts": completed_orders_count,
                "failedSlots": Vec::<Value>::new(),
                "inFlight": Vec::<usize>::new(),
                "maxConcurrent": queue_concurrency,
                "targetSourceOrders": target_source_orders,
                "queuedSourceOrders": source_orders_from_prompts(&queued_prompts),
                "currentBatchSourceOrders": if mode == "IMAGE_TO_VIDEO" {
                    source_orders_from_prompts(&queued_prompts).into_iter().take(queue_concurrency).collect::<Vec<usize>>()
                } else {
                    Vec::<usize>::new()
                },
                "phase": if mode == "IMAGE_TO_VIDEO" { IMAGE_TO_VIDEO_PHASE_GENERATE } else { "" },
                "remainingPrompts": total_prompts.saturating_sub(completed_orders_count)
            }
        }),
    )?;

    println!(
        "[Queue] Iniciando geracao do projeto {}: {} prompt(s) pendente(s), modo={}, concorrencia={}, estrategia={}, continue={}",
        local_project_id,
        total_prompts,
        mode,
        queue_concurrency,
        generation_strategy,
        can_continue
    );

    pump_generation_queue(&bridge);

    Ok(if generation_strategy == "restart" {
        format!(
            "Nova geracao iniciada do zero. {} prompt(s) na fila com limite global de {} em voo.",
            total_prompts,
            queue_concurrency
        )
    } else {
        format!(
            "Continuacao iniciada. {} prompt(s) pendente(s) com limite global de {} em voo.",
            total_prompts.saturating_sub(completed_orders_count),
            queue_concurrency
        )
    })
}

#[tauri::command]
fn get_generation_progress(
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
) -> Result<GenerationProgress, String> {
    require_auth(&auth)?;
    let mut queue = bridge
        .generation_queue
        .lock()
        .map_err(|_| "Nao foi possivel acessar o progresso da geracao.".to_string())?;
    if let Some(q) = queue.as_mut() {
        let _ = reconcile_live_generation_queue(q)?;
    }
    match &*queue {
        Some(q) => {
            let (total_prompts, completed_prompts, failed_slots, current_index) =
                local_generation_progress_snapshot(q).unwrap_or_else(|_| {
                    (
                        q.total_prompts,
                        q.completed_assets.len(),
                        q.failed_slots
                            .iter()
                            .map(|(order, err)| json!({"sourceOrder": order, "error": err}))
                            .collect(),
                        q.next_index,
                    )
                });
            Ok(GenerationProgress {
                local_project_id: Some(q.local_project_id.clone()),
                active: q.active,
                total_prompts,
                completed_prompts,
                failed_slots,
                current_index,
                in_flight: q.in_flight.len(),
                paused: q.paused,
            })
        }
        None => Ok(GenerationProgress {
            local_project_id: None,
            active: false,
            total_prompts: 0,
            completed_prompts: 0,
            failed_slots: vec![],
            current_index: 0,
            in_flight: 0,
            paused: false,
        }),
    }
}

#[tauri::command]
fn pause_project_generation(
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    local_project_id: String,
) -> Result<String, String> {
    require_auth(&auth)?;
    let mut queue = bridge
        .generation_queue
        .lock()
        .map_err(|_| "Nao foi possivel acessar a fila de geracao.".to_string())?;
    let q = queue
        .as_mut()
        .ok_or_else(|| "Nenhuma fila de geracao carregada.".to_string())?;
    if q.local_project_id != local_project_id {
        return Err("A fila carregada pertence a outro projeto.".to_string());
    }
    q.active = false;
    q.paused = true;
    persist_generation_queue_state(q);
    Ok("Geracao pausada neste projeto.".to_string())
}

#[tauri::command]
fn retry_failed_generations(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    local_project_id: String,
) -> Result<String, String> {
    require_auth(&auth)?;
    let failed_prompts: Vec<Value>;

    {
        let mut queue = bridge
            .generation_queue
            .lock()
            .map_err(|_| "Nao foi possivel acessar a fila de geracao.".to_string())?;
        let q = queue
            .as_mut()
            .ok_or_else(|| "Nenhuma geracao em andamento ou anterior.".to_string())?;
        if q.failed_slots.is_empty() {
            return Err("Nao ha slots com falha para retentar.".to_string());
        }
        if q.local_project_id != local_project_id {
            return Err("O projeto informado nao corresponde a geracao atual.".to_string());
        }

        let registry = read_registry(&app)?;
        let entry = registry
            .projects
            .iter()
            .find(|e| e.local_project_id == local_project_id)
            .ok_or_else(|| "Producao nao registrada.".to_string())?;
        let project_root = validate_project_root(&app, &entry.project_root)?;
        let all_prompts = read_json(&project_root.join("prompts").join("ordered-prompts.json"))?
            .get("prompts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let all_slots = read_json(&project_root.join(".flowcontent").join("production.json"))?
            .get("generationSlots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let failed_orders: Vec<usize> = q.failed_slots.iter().map(|(o, _)| *o).collect();
        failed_prompts = if q.mode == "ANIMATE_IMAGES" {
            build_animation_queue_items(&all_prompts, &all_slots, Some(failed_orders.as_slice()))
        } else {
            all_prompts
                .into_iter()
                .filter(|p| {
                    p.get("sourceOrder")
                        .and_then(Value::as_u64)
                        .map(|o| failed_orders.contains(&(o as usize)))
                        .unwrap_or(false)
                })
                .collect()
        };

        // Reset failed and rebuild prompts list
        q.failed_slots.clear();
        q.prompts = failed_prompts.clone();
        q.all_prompts = failed_prompts.clone();
        q.phase = if q.mode == "IMAGE_TO_VIDEO" {
            IMAGE_TO_VIDEO_PHASE_GENERATE.to_string()
        } else {
            q.mode.clone()
        };
        q.next_index = 0;
        q.active = true;
        q.paused = false;
        q.total_prompts = failed_prompts.len();
        q.in_flight.clear();
        q.target_source_orders = failed_prompts
            .iter()
            .filter_map(|prompt| {
                prompt
                    .get("sourceOrder")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
            })
            .collect();
        q.current_batch_source_orders = if q.mode == "IMAGE_TO_VIDEO" {
            source_orders_from_prompts(&failed_prompts)
                .into_iter()
                .take(q.max_concurrent)
                .collect()
        } else {
            q.target_source_orders.clone()
        };
        if q.mode == "IMAGE_TO_VIDEO" {
            initialize_image_to_video_batch(q);
        }
        persist_generation_queue_state(q);
    }

    if failed_prompts.is_empty() {
        return Err("Nenhum prompt com falha encontrado.".to_string());
    }

    pump_generation_queue(&bridge);

    Ok(format!(
        "Retentando {} prompts com falha.",
        failed_prompts.len()
    ))
}

#[tauri::command]
fn queue_project_animation(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    local_project_id: String,
    source_orders: Option<Vec<usize>>,
    i2v_model: Option<String>,
    video_aspect_ratio: Option<String>,
    max_concurrent: Option<usize>,
) -> Result<String, String> {
    require_auth(&auth)?;
    let registry = read_registry(&app)?;
    let entry = registry
        .projects
        .iter()
        .find(|entry| entry.local_project_id == local_project_id)
        .ok_or_else(|| "Producao nao registrada.".to_string())?;
    let project_root = validate_project_root(&app, &entry.project_root)?;
    if let Ok(queue) = bridge.generation_queue.lock() {
        if let Some(active_queue) = queue.as_ref() {
            if active_queue.active {
                return Err(format!(
                    "Ja existe uma geracao ativa no projeto {}. Aguarde a fila atual terminar antes de animar slots.",
                    active_queue.local_project_id
                ));
            }
        }
    }
    let flow_project_id = entry
        .flow_project_id
        .clone()
        .ok_or_else(|| "Aguarde o vinculo automatico com o projeto Flow.".to_string())?;
    let production_path = project_root.join(".flowcontent").join("production.json");
    let mut production = read_json(&production_path)?;
    if reconcile_production_with_downloads(
        &project_root,
        &mut production,
        &scan_downloaded_assets(&project_root),
    ) {
        write_json(&production_path, &production)?;
    }
    let prompts = read_json(&project_root.join("prompts").join("ordered-prompts.json"))?
        .get("prompts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let queued_prompts = build_animation_queue_items(&prompts, &slots, source_orders.as_deref());
    if queued_prompts.is_empty() {
        return Err("Nao ha imagens prontas para animar neste projeto.".to_string());
    }

    let settings = production
        .get("generationSettings")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let animation_model = i2v_model
        .or_else(|| {
            settings
                .get("i2vModel")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "veo_3_1_i2v_lite_low_priority".to_string());
    let video_ratio = video_aspect_ratio
        .or_else(|| {
            settings
                .get("videoAspectRatio")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "VIDEO_ASPECT_RATIO_LANDSCAPE".to_string());
    let image_model = settings
        .get("imageModel")
        .and_then(Value::as_str)
        .unwrap_or("GEM_PIX_2")
        .to_string();
    let image_ratio = settings
        .get("imageAspectRatio")
        .and_then(Value::as_str)
        .unwrap_or("IMAGE_ASPECT_RATIO_LANDSCAPE")
        .to_string();
    let target_source_orders: Vec<usize> = queued_prompts
        .iter()
        .filter_map(|prompt| {
            prompt
                .get("sourceOrder")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .collect();
    let queue_concurrency = normalized_queue_concurrency(max_concurrent.unwrap_or(2));

    for source_order in &target_source_orders {
        if let Some(slot) = slots.iter().find(|slot| {
            slot.get("sourceOrder").and_then(Value::as_u64) == Some(*source_order as u64)
        }) {
            let current_file_type = slot
                .get("currentFileType")
                .and_then(Value::as_str)
                .unwrap_or("image");
            let local_path = slot.get("localPath").cloned().unwrap_or(Value::Null);
            let remote_url = slot.get("remoteUrl").cloned().unwrap_or(Value::Null);
            let image_media_id = slot_image_media_id(slot);
            let _ = update_generation_slot(
                &project_root,
                *source_order,
                json!({
                    "status": "processing",
                    "assetType": "video",
                    "currentFileType": current_file_type,
                    "localPath": local_path,
                    "remoteUrl": remote_url,
                    "imageMediaId": image_media_id,
                    "error": Value::Null
                }),
            );
        }
    }

    {
        let mut queue = bridge
            .generation_queue
            .lock()
            .map_err(|_| "Nao foi possivel acessar a fila de geracao.".to_string())?;
        *queue = Some(GenerationQueueState {
            active: true,
            paused: false,
            local_project_id: local_project_id.clone(),
            flow_project_id: flow_project_id.clone(),
            project_root: project_root.clone(),
            mode: "ANIMATE_IMAGES".to_string(),
            image_model,
            video_model: animation_model.clone(),
            i2v_model: animation_model.clone(),
            image_aspect_ratio: image_ratio,
            video_aspect_ratio: video_ratio.clone(),
            prompts: queued_prompts.clone(),
            all_prompts: queued_prompts.clone(),
            phase: "ANIMATE_IMAGES".to_string(),
            next_index: 0,
            completed_assets: vec![],
            failed_slots: vec![],
            total_prompts: queued_prompts.len(),
            max_concurrent: queue_concurrency,
            target_source_orders: target_source_orders.clone(),
            current_batch_source_orders: target_source_orders.clone(),
            in_flight: HashMap::new(),
        });
    }

    update_production(
        &project_root,
        json!({
            "stage": "GENERATING_ASSETS",
            "generationMode": "ANIMATE_IMAGES",
            "generationTotalPrompts": queued_prompts.len(),
            "generationSettings": {
                "imageModel": settings.get("imageModel").and_then(Value::as_str).unwrap_or("GEM_PIX_2"),
                "videoModel": settings.get("videoModel").and_then(Value::as_str).unwrap_or("veo_3_1_t2v_lite_low_priority"),
                "i2vModel": animation_model,
                "imageAspectRatio": settings.get("imageAspectRatio").and_then(Value::as_str).unwrap_or("IMAGE_ASPECT_RATIO_LANDSCAPE"),
                "videoAspectRatio": video_ratio
            },
            "generationState": {
                "active": true,
                "paused": false,
                "mode": "ANIMATE_IMAGES",
                "nextIndex": 0,
                "completedPrompts": 0,
                "failedSlots": Vec::<Value>::new(),
                "inFlight": Vec::<usize>::new(),
                "maxConcurrent": queue_concurrency,
                "targetSourceOrders": target_source_orders,
                "queuedSourceOrders": source_orders_from_prompts(&queued_prompts),
                "currentBatchSourceOrders": source_orders_from_prompts(&queued_prompts),
                "phase": "ANIMATE_IMAGES",
                "remainingPrompts": queued_prompts.len()
            }
        }),
    )?;

    pump_generation_queue(&bridge);

    Ok(format!(
        "Animacao iniciada para {} slot(s), com limite global de {} em voo.",
        queued_prompts.len(),
        queue_concurrency
    ))
}

#[tauri::command]
fn reconcile_project_slot_asset(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    local_project_id: String,
    source_order: usize,
) -> Result<String, String> {
    require_auth(&auth)?;
    let registry = read_registry(&app)?;
    let entry = registry
        .projects
        .iter()
        .find(|entry| entry.local_project_id == local_project_id)
        .ok_or_else(|| "Producao nao registrada.".to_string())?;
    let project_root = validate_project_root(&app, &entry.project_root)?;
    let production_path = project_root.join(".flowcontent").join("production.json");
    let production = read_json(&production_path)?;
    let slots = production
        .get("generationSlots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let slot = slots
        .iter()
        .find(|slot| slot_source_order(slot) == Some(source_order))
        .cloned()
        .unwrap_or_else(|| json!({ "sourceOrder": source_order }));

    let (path, file_type) =
        locate_downloaded_asset(&project_root, source_order).ok_or_else(|| {
            format!(
                "Nenhum arquivo encontrado em downloads para o slot {}.",
                source_order
            )
        })?;

    let existing_asset_type = slot
        .get("assetType")
        .and_then(Value::as_str)
        .unwrap_or(file_type);
    let resolved_media_id = slot.get("mediaId").cloned().unwrap_or(Value::Null);
    let resolved_image_media_id = slot_image_media_id(&slot)
        .map(Value::String)
        .unwrap_or(Value::Null);
    let resolved_remote_url = slot.get("remoteUrl").cloned().unwrap_or(Value::Null);
    let status = if file_type == "video" {
        "ready"
    } else if existing_asset_type == "video" {
        "image-ready"
    } else {
        "ready"
    };

    update_generation_slot(
        &project_root,
        source_order,
        json!({
            "status": status,
            "assetType": if file_type == "video" { "video" } else { existing_asset_type },
            "currentFileType": file_type,
            "localPath": path.to_string_lossy().to_string(),
            "mediaId": resolved_media_id,
            "imageMediaId": resolved_image_media_id,
            "remoteUrl": resolved_remote_url,
            "error": Value::Null
        }),
    )?;

    Ok(format!(
        "Slot {} sincronizado novamente a partir de downloads.",
        source_order
    ))
}

#[tauri::command]
fn read_local_image_data_url(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    local_path: PathBuf,
) -> Result<String, String> {
    require_auth(&auth)?;
    let normalized = if local_path.to_string_lossy().starts_with(r"\\?\") {
        PathBuf::from(local_path.to_string_lossy().trim_start_matches(r"\\?\"))
    } else {
        local_path
    };
    let bytes = fs::read(&normalized)
        .map_err(|error| format!("Nao foi possivel ler a imagem local: {error}"))?;
    let mime = image_mime_from_path(&normalized);
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let _ = app;
    Ok(format!("data:{mime};base64,{encoded}"))
}

#[tauri::command]
fn get_slot_video_preview_data_url(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    local_project_id: String,
    source_order: usize,
) -> Result<String, String> {
    require_auth(&auth)?;
    let registry = read_registry(&app)?;
    let entry = registry
        .projects
        .iter()
        .find(|entry| entry.local_project_id == local_project_id)
        .ok_or_else(|| "Producao nao registrada.".to_string())?;
    let project_root = validate_project_root(&app, &entry.project_root)?;
    let preview_path = ensure_video_preview_image(&project_root, source_order)?;
    let bytes = fs::read(&preview_path)
        .map_err(|error| format!("Nao foi possivel ler a thumbnail do video: {error}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

#[tauri::command]
fn read_local_video_blob_payload(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    local_path: PathBuf,
) -> Result<Value, String> {
    require_auth(&auth)?;
    let normalized = if local_path.to_string_lossy().starts_with(r"\\?\") {
        PathBuf::from(local_path.to_string_lossy().trim_start_matches(r"\\?\"))
    } else {
        local_path
    };
    let bytes = fs::read(&normalized)
        .map_err(|error| format!("Nao foi possivel ler o video local: {error}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mime = match normalized
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        _ => "video/mp4",
    };
    let _ = app;
    Ok(json!({
        "mimeType": mime,
        "base64": encoded
    }))
}

#[tauri::command]
async fn process_audio(
    app: tauri::AppHandle,
    auth: State<'_, AuthState>,
    project_root: PathBuf,
    audio_path: PathBuf,
    asset_mode: String,
    asset_value: u16,
    transition_mode: String,
) -> Result<Value, String> {
    require_auth(&auth)?;
    let project_root = validate_project_root(&app, &project_root)?;
    if !audio_path.is_file() {
        return Err("Selecione um arquivo de audio valido.".to_string());
    }
    if !["words", "seconds", "pause"].contains(&asset_mode.as_str()) {
        return Err("Modo de segmentacao SRT invalido.".to_string());
    }
    if asset_value == 0
        || (asset_mode == "seconds" && asset_value > 8)
        || (asset_mode == "pause" && asset_value > 10_000)
    {
        return Err("As configuracoes de segmentacao SRT sao invalidas.".to_string());
    }
    if asset_mode == "pause"
        && !["midpoint", "next-speech", "previous-speech"].contains(&transition_mode.as_str())
    {
        return Err("Modo de transicao invalido.".to_string());
    }

    let script_path = bundled_or_dev_path(&app, "srt/flowcontent_srt.py")
        .map_err(|error| format!("Nao foi possivel localizar o processador SRT: {error}"))?;
    let api_key_file = assemblyai_key_file(&app)?;
    if !api_key_file.is_file() {
        return Err(
            "Configure a chave da AssemblyAI em Sessoes Flow antes de processar o audio."
                .to_string(),
        );
    }
    let audio_path_for_cmd = audio_path.clone();
    let project_root_for_cmd = project_root.clone();
    let transition_mode_for_cmd = transition_mode.clone();
    let asset_mode_for_cmd = asset_mode.clone();
    let api_key_file_for_cmd = api_key_file.clone();
    let audio_file_name = audio_path
        .file_name()
        .ok_or_else(|| "Nao foi possivel identificar o nome do audio.".to_string())?
        .to_os_string();
    let audio_file_stem = audio_path
        .file_stem()
        .ok_or_else(|| "Nao foi possivel identificar o nome base do audio.".to_string())?
        .to_os_string();
    let output = tauri::async_runtime::spawn_blocking(move || {
        let mut failures = Vec::new();
        for candidate in python_command_candidates() {
            let mut command = Command::new(&candidate.program);
            command.args(&candidate.prefix_args);
            command
                .env("PYTHONUTF8", "1")
                .env("PYTHONIOENCODING", "utf-8")
                .arg(&script_path)
                .arg("--audio")
                .arg(&audio_path_for_cmd)
                .arg("--project-root")
                .arg(&project_root_for_cmd)
                .arg("--asset-mode")
                .arg(&asset_mode_for_cmd)
                .arg("--asset-value")
                .arg(asset_value.to_string())
                .arg("--pause-ms")
                .arg(if asset_mode_for_cmd == "pause" {
                    asset_value.to_string()
                } else {
                    "100".to_string()
                })
                .arg("--transition-mode")
                .arg(&transition_mode_for_cmd)
                .arg("--api-key-file")
                .arg(&api_key_file_for_cmd);
            match command.output() {
                Ok(output) => return Ok(output),
                Err(error) => {
                    failures.push(format!("{} ({error})", candidate.program.to_string_lossy()))
                }
            }
        }
        Err(format!(
            "Nao foi possivel localizar um interpretador Python executavel. Tentativas: {}",
            failures.join(", ")
        ))
    })
    .await
    .map_err(|error| format!("Falha ao aguardar o processador de audio: {error}"))??;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        return Err(message.trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: Value = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("Resposta invalida do processador SRT: {error}"))?;
    let project_audio_path = project_root.join("audio").join(&audio_file_name);
    let caption_srt_path = project_root.join("srt").join(format!(
        "{}.legendas.srt",
        PathBuf::from(&audio_file_stem).to_string_lossy()
    ));
    let asset_srt_path = project_root.join("srt").join(format!(
        "{}.assets.srt",
        PathBuf::from(&audio_file_stem).to_string_lossy()
    ));
    let manifest_path = project_root
        .join(".flowcontent")
        .join("audio-segments.json");
    update_production(
        &project_root,
        json!({
            "stage": "AWAITING_PROMPTS",
            "audioPath": project_audio_path,
            "captionSrtPath": caption_srt_path,
            "assetSrtPath": asset_srt_path,
            "manifestPath": manifest_path,
            "assetCount": result.get("assetCount"),
            "captionCount": result.get("captionCount"),
            "languageCode": result.get("languageCode"),
            "settings": {
                "captionMaxWords": if asset_mode == "words" { u32::from(asset_value) } else { 7 },
                "pauseThresholdMs": if asset_mode == "pause" { u32::from(asset_value) } else { 100 },
                "transitionMode": transition_mode,
                "assetMinDurationMs": 3000,
                "assetMaxDurationMs": result.get("maxAssetDurationMs").cloned().unwrap_or(json!(8000)),
                "assetSegmentationMode": asset_mode,
                "assetSegmentationValue": asset_value
            }
        }),
    )?;
    Ok(result)
}

#[tauri::command]
fn import_prompts(
    app: tauri::AppHandle,
    auth: State<AuthState>,
    bridge: State<FlowBridgeState>,
    project_root: PathBuf,
    prompts: Vec<String>,
) -> Result<Value, String> {
    require_auth(&auth)?;
    let project_root = validate_project_root(&app, &project_root)?;
    let cleaned: Vec<String> = prompts
        .into_iter()
        .map(|prompt| prompt.trim().to_string())
        .filter(|prompt| !prompt.is_empty())
        .collect();
    if cleaned.is_empty() {
        return Err("Informe pelo menos um prompt.".to_string());
    }

    let segments_path = project_root
        .join(".flowcontent")
        .join("audio-segments.json");
    let segments = if segments_path.exists() {
        read_json(&segments_path)?
    } else {
        json!({})
    };
    let assets = segments.get("assets").and_then(Value::as_array);
    if let Some(assets) = assets {
        if cleaned.len() != assets.len() {
            return Err(format!(
                "Quantidade incorreta: a producao possui {} slots e voce enviou {} prompts.",
                assets.len(),
                cleaned.len()
            ));
        }
    }

    let assignments: Vec<Value> = cleaned
        .iter()
        .enumerate()
        .map(|(index, prompt)| {
            let asset = assets.and_then(|items| items.get(index));
            json!({
                "sourceOrder": index + 1,
                "assetBlockId": asset.and_then(|value| value.get("segment_id")).cloned().unwrap_or_else(|| json!(format!("manual-{}", index + 1))),
                "startMs": asset.and_then(|value| value.get("start")).cloned().unwrap_or(Value::Null),
                "endMs": asset.and_then(|value| value.get("end")).cloned().unwrap_or(Value::Null),
                "focusText": asset.and_then(|value| value.get("text")).cloned().unwrap_or_else(|| json!("")),
                "contextText": asset.and_then(|value| value.get("context_text")).cloned().unwrap_or_else(|| json!("")),
                "prompt": prompt
            })
        })
        .collect();
    let output_path = project_root.join("prompts").join("ordered-prompts.json");
    write_json(
        &output_path,
        &json!({
            "version": 1,
            "count": assignments.len(),
            "prompts": assignments,
            "updatedAt": now_string()
        }),
    )?;
    if let Ok(mut queue) = bridge.generation_queue.lock() {
        if queue
            .as_ref()
            .is_some_and(|state| state.project_root == project_root)
        {
            *queue = None;
        }
    }
    update_production(
        &project_root,
        json!({
            "stage": "READY_FOR_FLOW",
            "promptPath": output_path,
            "promptCount": assignments.len(),
            "assetCount": assignments.len(),
            "generationSlots": build_generation_slots(&assignments, "IMAGE"),
            "generationMode": Value::Null,
            "generationSettings": Value::Null,
            "generationState": {
                "active": false,
                "paused": false,
                "mode": Value::Null,
                "nextIndex": 0,
                "completedPrompts": 0,
                "failedSlots": Vec::<Value>::new(),
                "inFlight": Vec::<usize>::new(),
                "maxConcurrent": normalized_queue_concurrency(2),
                "remainingPrompts": assignments.len()
            },
            "remoteMediaStoredLocally": false
        }),
    )?;
    Ok(json!({ "count": assignments.len(), "promptPath": output_path }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let bridge = FlowBridgeState::new();
    start_bridge_server(bridge.clone()).expect("failed to start local Flow bridge");
    tauri::Builder::default()
        .manage(AuthState::default())
        .manage(DiagnosticState::default())
        .manage(PendingUpdate::default())
        .manage(bridge.clone())
        .setup(|app| {
            let bridge = app.state::<FlowBridgeState>();
            if let Ok(mut guard) = bridge.app_handle.lock() {
                *guard = Some(app.handle().clone());
            }
            let _ = restore_generation_queue(&app.handle().clone(), &bridge);
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
                .map_err(|error| format!("Falha ao iniciar o updater: {error}"))?;
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            record_diagnostic_event,
            get_auth_status,
            validate_license,
            get_saved_license,
            authenticate,
            lock_app,
            get_assemblyai_status,
            save_assemblyai_keys,
            clear_assemblyai_keys,
            get_runtime_info,
            get_update_status,
            check_for_update,
            install_pending_update,
            initialize_workspace,
            get_flow_bridge_status,
            open_flow_browser,
            sync_flow_project_links,
            ensure_flow_project_link,
            list_projects,
            get_project_detail,
            create_project,
            delete_project,
            export_project_srt,
            export_capcut_project,
            process_audio,
            import_prompts,
            queue_project_generation,
            pause_project_generation,
            queue_project_animation,
            reconcile_project_slot_asset,
            read_local_image_data_url,
            get_slot_video_preview_data_url,
            read_local_video_blob_payload,
            get_generation_progress,
            retry_failed_generations
        ])
        .run(tauri::generate_context!())
        .expect("error while running FlowContent Auto");
}

#[cfg(test)]
mod tests {
    use super::{
        extension_installed_in_profile, handle_bridge_request, token_matches,
        workspace_root_usable, FlowBridgeState,
    };
    use serde_json::json;
    use std::{
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };
    use uuid::Uuid;

    #[test]
    fn accepts_the_configured_development_token() {
        assert!(token_matches("CF-DEV-TEST-2024"));
    }

    #[test]
    fn rejects_a_different_token() {
        assert!(!token_matches("CF-DEV-TEST-2025"));
    }

    #[test]
    fn detects_an_extension_installed_in_the_dedicated_profile() {
        let root = std::env::temp_dir().join(format!("flowcontent-test-{}", Uuid::new_v4()));
        let extension_path = root.join("flow-bridge-extension");
        let preferences_path = root
            .join("profile")
            .join("Default")
            .join("Secure Preferences");
        fs::create_dir_all(preferences_path.parent().unwrap()).unwrap();
        fs::write(
            &preferences_path,
            serde_json::to_vec(&json!({
                "extensions": {
                    "settings": {
                        "extension-id": { "path": extension_path }
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(extension_installed_in_profile(
            &root.join("profile"),
            &extension_path
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_an_existing_workspace_root() {
        let root = std::env::temp_dir().join(format!("flowcontent-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        assert!(workspace_root_usable(&root));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_a_workspace_root_when_the_parent_exists() {
        let parent = std::env::temp_dir().join(format!("flowcontent-test-{}", Uuid::new_v4()));
        let child = parent.join("workspace");
        fs::create_dir_all(&parent).unwrap();

        assert!(workspace_root_usable(&child));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn rejects_a_workspace_root_when_the_parent_is_missing() {
        let missing = std::env::temp_dir()
            .join(format!("flowcontent-test-{}", Uuid::new_v4()))
            .join("workspace");

        assert!(!workspace_root_usable(&missing));
    }

    #[test]
    fn bridge_reads_a_body_delivered_after_the_headers() {
        let bridge = FlowBridgeState::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_bridge = bridge.clone();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_bridge_request(stream, &server_bridge);
        });

        let body = json!({
            "pageDetected": true,
            "url": "https://labs.google/fx/pt/tools/flow/project/985209ec-1e76-455c-b93b-88b6f8b60750",
            "title": "Google Flow",
            "projectId": "985209ec-1e76-455c-b93b-88b6f8b60750"
        })
        .to_string();
        let headers = format!(
            "POST /heartbeat HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nX-FlowContent-Bridge: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bridge.token,
            body.len()
        );
        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(headers.as_bytes()).unwrap();
        client.flush().unwrap();
        thread::sleep(Duration::from_millis(20));
        client.write_all(body.as_bytes()).unwrap();
        client.flush().unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert_eq!(
            bridge.status.lock().unwrap().project_id.as_deref(),
            Some("985209ec-1e76-455c-b93b-88b6f8b60750")
        );
    }
}
