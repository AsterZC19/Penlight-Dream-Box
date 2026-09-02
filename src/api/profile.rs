//! Bestdori Profile Manager export.
//!
//! Bestdori accepts an uncompressed profile object through its Import panel.
//! This module converts the decoded player data exposed by Penlight-Dream-API
//! into that format. The endpoint only generates a file; it does not attempt
//! to access Bestdori cookies or browser localStorage.

use std::collections::HashSet;

use axum::body::Body;
use axum::extract::{Json, State};
use axum::http::header::{
    HeaderValue, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::api::AppState;
use crate::error::{AppError, AppResult};
use crate::garupa::Credentials;

const ITEM_GROUPS: &[(&str, usize)] = &[
    ("PoppinParty", 7),
    ("Afterglow", 7),
    ("HelloHappyWorld", 7),
    ("PastelPalettes", 7),
    ("Roselia", 7),
    ("RaiseASuilen", 7),
    ("Morfonica", 7),
    ("MyGO", 7),
    ("Everyone", 7),
    ("Magazine", 3),
    ("Plaza", 4),
    ("Menu", 4),
];

// These are area-item category IDs, in the same slot order used by
// Bestdori's Profile Manager. Categories 59, 68 and 72 are present in the
// game snapshot but have no corresponding current Bestdori import slot.
const ITEM_CATEGORY_SLOTS: &[(&str, &[i64])] = &[
    ("PoppinParty", &[1, 2, 3, 4, 5, 6, 7]),
    ("Afterglow", &[8, 9, 10, 11, 12, 13, 14]),
    ("HelloHappyWorld", &[15, 16, 17, 18, 19, 20, 21]),
    ("PastelPalettes", &[22, 23, 24, 25, 26, 27, 28]),
    ("Roselia", &[29, 30, 31, 32, 33, 34, 35]),
    ("RaiseASuilen", &[90, 91, 92, 93, 94, 95, 96]),
    ("Morfonica", &[83, 84, 85, 86, 87, 88, 89]),
    ("MyGO", &[97, 98, 99, 100, 101, 102, 103]),
    ("Everyone", &[73, 74, 75, 76, 77, 78, 79]),
    ("Magazine", &[80, 81, 82]),
    ("Plaza", &[70, 66, 67, 69]),
    ("Menu", &[56, 57, 58, 60]),
];

/// Generates a Bestdori Profile Manager import file for the configured player.
pub async fn export(State(state): State<AppState>) -> AppResult<Response> {
    let (profile, situations, episodes, cards, areas, characters) = tokio::try_join!(
        state.upstream.user_profile(),
        state.upstream.user_situations(),
        state.upstream.user_episodes(),
        state.upstream.cards(),
        state.upstream.user_areas(),
        state.upstream.user_characters(),
    )?;

    let profile = build_profile_with_user_data(
        &profile,
        &situations,
        &episodes,
        &cards,
        &areas,
        &characters,
    );
    profile_response(&profile, "bestdori-profile.json")
}

/// Credentials are request-scoped. They are validated, used to call the
/// official API, and then dropped; they are not written to MongoDB or a cache.
#[derive(Deserialize)]
pub struct ProfileRequest {
    #[serde(alias = "userId")]
    pub uid: String,
    #[serde(alias = "deviceUuid")]
    pub uuid: String,
    #[serde(alias = "clientPlatform")]
    pub platform: String,
}

/// Generates a Bestdori profile for the UID/UUID/platform submitted by the UI.
pub async fn export_for_credentials(
    State(state): State<AppState>,
    Json(request): Json<ProfileRequest>,
) -> AppResult<Response> {
    let credentials = Credentials::from_input(&request.uid, &request.uuid, &request.platform)?;
    let snapshot = state.profile_client.fetch(&credentials).await?;
    let profile = build_profile_with_user_data(
        &snapshot.profile,
        &snapshot.situations,
        &snapshot.episodes,
        &snapshot.cards,
        &snapshot.areas,
        &snapshot.characters,
    );
    let filename = format!("bestdori-profile-{}.json", credentials.uid);
    profile_response(&profile, &filename)
}

fn profile_response(profile: &Value, filename: &str) -> AppResult<Response> {
    let body = serde_json::to_string_pretty(profile)
        .map_err(|e| AppError::internal(format!("failed to serialize Bestdori profile: {e}")))?;
    let disposition = format!("attachment; filename=\"{filename}\"");
    let disposition = HeaderValue::from_str(&disposition)
        .map_err(|_| AppError::internal("failed to build profile download headers"))?;

    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(CONTENT_DISPOSITION, disposition);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// Converts Dream-API responses to the uncompressed profile shape understood
/// by Bestdori's current importer. Keeping this pure makes the mapping easy to
/// test without a live game account.
#[cfg(test)]
pub(crate) fn build_profile(
    profile: &Value,
    situations: &Value,
    episodes: &Value,
    card_master: &Value,
) -> Value {
    build_profile_with_user_data(
        profile,
        situations,
        episodes,
        card_master,
        &json!({ "entries": [] }),
        &json!({ "entries": [] }),
    )
}

fn build_profile_with_user_data(
    profile: &Value,
    situations: &Value,
    episodes: &Value,
    card_master: &Value,
    areas: &Value,
    characters: &Value,
) -> Value {
    let uid = first_i64(profile, &["profile.userId", "stats.userId", "userId"]);
    let name = first_string(profile, &["profile.userName", "userName", "name"])
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if uid > 0 {
                format!("Garupa {uid}")
            } else {
                "Garupa Profile".to_string()
            }
        });

    let unlocked_episodes = episode_ids(episodes);
    let cards = situations
        .get("entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| card_entry(entry, &unlocked_episodes, card_master))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "name": name,
        "server": 0,
        "items": items_from_user_data(areas, characters),
        "cards": cards,
    })
}

fn card_entry(
    entry: &Value,
    unlocked_episodes: &HashSet<i64>,
    card_master: &Value,
) -> Option<Value> {
    let id = i64_field(entry, "situationId");
    if id < 1 {
        return None;
    }

    let training_status = string_field(entry, "trainingStatus").to_ascii_lowercase();
    let trained = matches!(
        training_status.as_str(),
        "completed" | "complete" | "trained" | "training_completed" | "after_training"
    );
    let after_training = string_field(entry, "illust") == "after_training";
    let trained = trained || after_training;

    Some(json!({
        "id": id,
        "level": non_negative(i64_field(entry, "level")),
        "master": non_negative(i64_field(entry, "limitBreakRank")),
        // Dream-API exposes the game's Lv.1..5 value. Bestdori stores the
        // zero-based value and displays it as `skill + 1`.
        "skill": non_negative(i64_field(entry, "skillLevel") - 1),
        "ep": episode_count(id, unlocked_episodes, card_master),
        "train": if trained { 1 } else { 0 },
        "art": if after_training { 1 } else { 0 },
        "exclude": false,
    }))
}

fn episode_ids(episodes: &Value) -> HashSet<i64> {
    episodes
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = i64_field(entry, "episodeId");
            (id > 0).then_some(id)
        })
        .collect()
}

fn episode_count(card_id: i64, unlocked_episodes: &HashSet<i64>, card_master: &Value) -> i64 {
    card_master
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|card| i64_field(card, "situationId") == card_id)
        .and_then(|card| card.get("episodes"))
        .and_then(|episodes| episodes.get("entries"))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|episode| unlocked_episodes.contains(&i64_field(episode, "episodeId")))
                .count() as i64
        })
        .unwrap_or(0)
}

fn default_items() -> Map<String, Value> {
    let mut items = Map::new();
    for (name, length) in ITEM_GROUPS {
        items.insert(
            (*name).to_string(),
            Value::Array(std::iter::repeat_n(Value::Null, *length).collect()),
        );
    }
    items.insert(
        "potentials".to_string(),
        Value::Array(std::iter::repeat_n(Value::from(1), 40).collect()),
    );
    items
}

fn items_from_user_data(areas: &Value, characters: &Value) -> Map<String, Value> {
    let mut items = default_items();

    if let Some(entries) = areas.get("entries").and_then(Value::as_array) {
        for entry in entries {
            let Some(category) = integer_value(entry.get("areaItemCategory")) else {
                continue;
            };
            let Some(level) = integer_value(entry.get("level")) else {
                continue;
            };
            let Some((name, categories)) = ITEM_CATEGORY_SLOTS
                .iter()
                .find(|(_, categories)| categories.contains(&category))
            else {
                continue;
            };
            let Some(index) = categories.iter().position(|value| *value == category) else {
                continue;
            };
            let Some(values) = items.get_mut(*name).and_then(Value::as_array_mut) else {
                continue;
            };
            values[index] = Value::from(non_negative(level - 1));
        }
    }

    if let Some(entries) = characters.get("entries").and_then(Value::as_array) {
        if let Some(potentials) = items.get_mut("potentials").and_then(Value::as_array_mut) {
            for rank in entries {
                let Some(character_id) = integer_value(rank.get("characterId"))
                    .and_then(|value| usize::try_from(value).ok())
                else {
                    continue;
                };
                if !(1..=potentials.len()).contains(&character_id) {
                    continue;
                }
                let Some(level) = character_potential_total(rank) else {
                    continue;
                };
                potentials[character_id - 1] = Value::from(non_negative(level));
            }
        }
    }

    items
}

fn character_potential_total(character: &Value) -> Option<i64> {
    if let Some(potential) = character.get("potentialLevel") {
        let level = ["performanceLevel", "techniqueLevel", "visualLevel"]
            .into_iter()
            .map(|field| integer_value(potential.get(field)).unwrap_or(0))
            .sum();
        return Some(level);
    }

    // Compatibility with the previous Dream-API response shape.
    integer_value(character.get("releasedPotentialLevel"))
}

fn first_i64(root: &Value, paths: &[&str]) -> i64 {
    paths
        .iter()
        .find_map(|path| root.pointer(&format!("/{}", path.replace('.', "/"))))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn first_string(root: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        root.pointer(&format!("/{}", path.replace('.', "/")))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    })
}

fn i64_field(root: &Value, field: &str) -> i64 {
    root.get(field).and_then(Value::as_i64).unwrap_or(0)
}

fn integer_value(value: Option<&Value>) -> Option<i64> {
    value.and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    })
}

fn string_field<'a>(root: &'a Value, field: &str) -> &'a str {
    root.get(field).and_then(Value::as_str).unwrap_or("")
}

fn non_negative(value: i64) -> i64 {
    value.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_owned_card_to_bestdori_fields_and_episode_count() {
        let profile = json!({
            "profile": { "userId": 12345, "userName": "Tester" },
            "stats": { "userId": 12345 }
        });
        let situations = json!({
            "entries": [{
                "situationId": 1001,
                "level": 60,
                "trainingStatus": "completed",
                "illust": "normal",
                "skillLevel": 5,
                "limitBreakRank": 2
            }]
        });
        let episodes = json!({
            "entries": [
                { "episodeId": 9001, "status": "read" },
                { "episodeId": 9002, "status": "read" }
            ]
        });
        let card_master = json!({
            "entries": [{
                "situationId": 1001,
                "episodes": { "entries": [
                    { "episodeId": 9001 },
                    { "episodeId": 9002 },
                    { "episodeId": 9003 }
                ]}
            }]
        });

        let result = build_profile(&profile, &situations, &episodes, &card_master);
        assert_eq!(result["name"], "Tester");
        assert_eq!(result["server"], 0);
        assert_eq!(result["cards"][0]["id"], 1001);
        assert_eq!(result["cards"][0]["level"], 60);
        assert_eq!(result["cards"][0]["master"], 2);
        assert_eq!(result["cards"][0]["skill"], 4);
        assert_eq!(result["cards"][0]["ep"], 2);
        assert_eq!(result["cards"][0]["train"], 1);
        assert_eq!(result["cards"][0]["art"], 0);
        assert_eq!(result["cards"][0]["exclude"], false);
        assert_eq!(result["items"]["potentials"].as_array().unwrap().len(), 40);
    }

    #[test]
    fn skips_invalid_card_ids_and_uses_uid_when_name_is_missing() {
        let result = build_profile(
            &json!({ "profile": { "userId": 77 } }),
            &json!({ "entries": [{ "situationId": 0 }, { "situationId": 5 }] }),
            &json!({ "entries": [] }),
            &json!({ "entries": [] }),
        );

        assert_eq!(result["name"], "Garupa 77");
        assert_eq!(result["cards"].as_array().unwrap().len(), 1);
        assert_eq!(result["cards"][0]["id"], 5);
    }

    #[test]
    fn maps_api_infrastructure_and_character_potential_levels() {
        let areas = json!({
            "entries": [
                { "areaItemCategory": 1, "level": 8 },
                { "areaItemCategory": 2, "level": "4" },
                { "areaItemCategory": 70, "level": "4" },
                { "areaItemCategory": 73, "level": 8 }
            ]
        });
        let characters = json!({
            "entries": [
                { "characterId": 1, "potentialLevel": {
                    "performanceLevel": 20,
                    "techniqueLevel": "15",
                    "visualLevel": 20
                }},
                { "characterId": 40, "potentialLevel": {
                    "performanceLevel": 50,
                    "techniqueLevel": 40,
                    "visualLevel": 30
                }}
            ]
        });

        let result = build_profile_with_user_data(
            &json!({ "profile": { "userId": 77 } }),
            &json!({ "entries": [] }),
            &json!({ "entries": [] }),
            &json!({ "entries": [] }),
            &areas,
            &characters,
        );

        assert_eq!(result["items"]["PoppinParty"][0], 7);
        assert_eq!(result["items"]["PoppinParty"][1], 3);
        assert_eq!(result["items"]["PoppinParty"][2], Value::Null);
        assert_eq!(result["items"]["Plaza"][0], 3);
        assert_eq!(result["items"]["Everyone"][0], 7);
        assert_eq!(result["items"]["Afterglow"][0], Value::Null);
        assert_eq!(result["items"]["potentials"][0], 55);
        assert_eq!(result["items"]["potentials"][39], 120);
    }
}
