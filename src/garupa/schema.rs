//! Protobuf schemas required by the profile export endpoints.

#[derive(Clone, Copy, Debug)]
pub enum ProtoType {
    Int,
    Long,
    String,
    Message(&'static Schema),
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub ty: ProtoType,
    pub repeated: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Schema {
    pub fields: &'static [(u32, Field)],
}

pub const fn field(name: &'static str, ty: ProtoType, repeated: bool) -> Field {
    Field { name, ty, repeated }
}

// User profile and owned cards.
static USER_PROFILE_STATS_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("userId", ProtoType::Long, false)),
        (2, field("rank", ProtoType::Int, false)),
        (3, field("level", ProtoType::Int, false)),
    ],
};

static USER_PROFILE_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("userId", ProtoType::Long, false)),
        (2, field("uuid", ProtoType::String, false)),
        (3, field("userName", ProtoType::String, false)),
        (4, field("clientVersion", ProtoType::String, false)),
        (5, field("platform", ProtoType::String, false)),
    ],
};

pub static USER_PROFILE_RESPONSE_SCHEMA: Schema = Schema {
    fields: &[
        (
            1,
            field("profile", ProtoType::Message(&USER_PROFILE_SCHEMA), false),
        ),
        (
            2,
            field(
                "stats",
                ProtoType::Message(&USER_PROFILE_STATS_SCHEMA),
                false,
            ),
        ),
    ],
};

static USER_APPEND_PARAMETER_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("userId", ProtoType::Long, false)),
        (2, field("situationId", ProtoType::Int, false)),
        (3, field("performance", ProtoType::Int, false)),
        (4, field("technique", ProtoType::Int, false)),
        (5, field("visual", ProtoType::Int, false)),
    ],
};

static USER_SITUATION_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("userId", ProtoType::Long, false)),
        (2, field("situationId", ProtoType::Int, false)),
        (3, field("level", ProtoType::Int, false)),
        (4, field("exp", ProtoType::Int, false)),
        (7, field("trainingStatus", ProtoType::String, false)),
        (9, field("illust", ProtoType::String, false)),
        (11, field("skillLevel", ProtoType::Int, false)),
        (
            12,
            field(
                "userAppendParameter",
                ProtoType::Message(&USER_APPEND_PARAMETER_SCHEMA),
                false,
            ),
        ),
        (13, field("limitBreakRank", ProtoType::Int, false)),
    ],
};

pub static USER_SITUATION_LIST_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field("entries", ProtoType::Message(&USER_SITUATION_SCHEMA), true),
    )],
};

static USER_EPISODE_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("userId", ProtoType::Long, false)),
        (2, field("episodeId", ProtoType::Int, false)),
        (3, field("status", ProtoType::String, false)),
    ],
};

pub static USER_EPISODE_LIST_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field("entries", ProtoType::Message(&USER_EPISODE_SCHEMA), true),
    )],
};

// Card master data.
static SITUATION_APPEND_PARAMETER_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("situationId", ProtoType::Int, false)),
        (2, field("level", ProtoType::Int, false)),
        (3, field("performance", ProtoType::Int, false)),
        (4, field("technique", ProtoType::Int, false)),
        (5, field("visual", ProtoType::Int, false)),
    ],
};

static SITUATION_LEVEL_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("level", ProtoType::Int, false)),
        (
            2,
            field(
                "appendParameter",
                ProtoType::Message(&SITUATION_APPEND_PARAMETER_SCHEMA),
                false,
            ),
        ),
    ],
};

static SITUATION_EPISODE_REWARD_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("rewardId", ProtoType::Int, false)),
        (2, field("rewardType", ProtoType::String, false)),
        (3, field("rewardQuantity", ProtoType::Int, false)),
    ],
};

static SITUATION_EPISODE_REWARD_LIST_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field(
            "entries",
            ProtoType::Message(&SITUATION_EPISODE_REWARD_SCHEMA),
            true,
        ),
    )],
};

static SITUATION_EPISODE_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("episodeId", ProtoType::Int, false)),
        (2, field("episodeType", ProtoType::String, false)),
        (3, field("episodeNumber", ProtoType::Int, false)),
        (4, field("assetBundleName", ProtoType::String, false)),
        (5, field("bonusPerformance", ProtoType::Int, false)),
        (6, field("bonusTechnique", ProtoType::Int, false)),
        (7, field("bonusVisual", ProtoType::Int, false)),
        (8, field("maxLevel", ProtoType::Int, false)),
        (
            9,
            field(
                "rewards",
                ProtoType::Message(&SITUATION_EPISODE_REWARD_LIST_SCHEMA),
                false,
            ),
        ),
        (
            10,
            field(
                "starRewards",
                ProtoType::Message(&SITUATION_EPISODE_REWARD_LIST_SCHEMA),
                false,
            ),
        ),
    ],
};

static SITUATION_EPISODE_LIST_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field(
            "entries",
            ProtoType::Message(&SITUATION_EPISODE_SCHEMA),
            true,
        ),
    )],
};

static SITUATION_TRAINING_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("situationId", ProtoType::Int, false)),
        (2, field("characterIndex", ProtoType::Int, false)),
        (3, field("level", ProtoType::Int, false)),
        (4, field("performance", ProtoType::Int, false)),
        (5, field("technique", ProtoType::Int, false)),
        (6, field("visual", ProtoType::Int, false)),
    ],
};

static SITUATION_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("situationId", ProtoType::Int, false)),
        (2, field("situationType", ProtoType::Int, false)),
        (3, field("rarity", ProtoType::Int, false)),
        (5, field("attribute", ProtoType::String, false)),
        (7, field("skillId", ProtoType::Int, false)),
        (
            8,
            field("levels", ProtoType::Message(&SITUATION_LEVEL_SCHEMA), true),
        ),
        (10, field("cardName", ProtoType::String, false)),
        (11, field("maxLevel", ProtoType::Int, false)),
        (12, field("resourceName", ProtoType::String, false)),
        (13, field("sdAssetName", ProtoType::String, false)),
        (
            14,
            field(
                "episodes",
                ProtoType::Message(&SITUATION_EPISODE_LIST_SCHEMA),
                false,
            ),
        ),
        (
            15,
            field(
                "training",
                ProtoType::Message(&SITUATION_TRAINING_SCHEMA),
                false,
            ),
        ),
        (16, field("characterIndex", ProtoType::Int, false)),
        (17, field("releaseAt", ProtoType::Long, false)),
        (18, field("skillId2", ProtoType::Int, false)),
        (19, field("flag2", ProtoType::Int, false)),
        (20, field("illustType", ProtoType::String, false)),
        (24, field("extra", ProtoType::String, false)),
        (25, field("seq", ProtoType::Int, false)),
    ],
};

pub static SITUATION_LIST_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field("entries", ProtoType::Message(&SITUATION_SCHEMA), true),
    )],
};

// Suite user snapshot: area item map and character potential map.
static USER_AREA_ITEM_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("userId", ProtoType::Long, false)),
        (2, field("areaItemId", ProtoType::Int, false)),
        (3, field("areaItemCategory", ProtoType::Int, false)),
        (4, field("level", ProtoType::Int, false)),
    ],
};

static USER_AREA_ITEM_MAP_ENTRY_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("key", ProtoType::Int, false)),
        (
            2,
            field("value", ProtoType::Message(&USER_AREA_ITEM_SCHEMA), false),
        ),
    ],
};

static USER_AREA_ITEM_MAP_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field(
            "entries",
            ProtoType::Message(&USER_AREA_ITEM_MAP_ENTRY_SCHEMA),
            true,
        ),
    )],
};

static USER_CHARACTER_POTENTIAL_LEVEL_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("performanceLevel", ProtoType::Int, false)),
        (2, field("techniqueLevel", ProtoType::Int, false)),
        (3, field("visualLevel", ProtoType::Int, false)),
    ],
};

static USER_CHARACTER_POTENTIAL_LEVEL_MAP_ENTRY_SCHEMA: Schema = Schema {
    fields: &[
        (1, field("key", ProtoType::Int, false)),
        (
            2,
            field(
                "value",
                ProtoType::Message(&USER_CHARACTER_POTENTIAL_LEVEL_SCHEMA),
                false,
            ),
        ),
    ],
};

static USER_CHARACTER_POTENTIAL_LEVEL_MAP_SCHEMA: Schema = Schema {
    fields: &[(
        1,
        field(
            "entries",
            ProtoType::Message(&USER_CHARACTER_POTENTIAL_LEVEL_MAP_ENTRY_SCHEMA),
            true,
        ),
    )],
};

pub static SUITE_USER_RESPONSE_SCHEMA: Schema = Schema {
    fields: &[
        (
            22,
            field(
                "userAreaItemMap",
                ProtoType::Message(&USER_AREA_ITEM_MAP_SCHEMA),
                false,
            ),
        ),
        (
            401,
            field(
                "userCharacterPotentialLevelMap",
                ProtoType::Message(&USER_CHARACTER_POTENTIAL_LEVEL_MAP_SCHEMA),
                false,
            ),
        ),
    ],
};
