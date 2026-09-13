use rockbox_playlists::{resolver, SmartPlaylist, TrackStats};
#[cfg(not(feature = "fts5"))]
use rockbox_typesense::client::{delete_playlist as ts_delete_playlist, insert_playlists};
#[cfg(not(feature = "fts5"))]
use rockbox_typesense::types::Playlist as TsPlaylist;

use crate::api::rockbox::v1alpha1::{
    smart_playlist_service_server::SmartPlaylistService, CreateSmartPlaylistRequest,
    CreateSmartPlaylistResponse, DeleteSmartPlaylistRequest, DeleteSmartPlaylistResponse,
    GetSmartPlaylistRequest, GetSmartPlaylistResponse, GetSmartPlaylistTracksRequest,
    GetSmartPlaylistTracksResponse, GetSmartPlaylistsRequest, GetSmartPlaylistsResponse,
    GetTrackStatsRequest, GetTrackStatsResponse, PlaySmartPlaylistRequest,
    PlaySmartPlaylistResponse, RecordTrackPlayedRequest, RecordTrackPlayedResponse,
    RecordTrackSkippedRequest, RecordTrackSkippedResponse, RuleCondition as ProtoRuleCondition,
    RuleCriteria as ProtoRuleCriteria, SmartPlaylist as ProtoSmartPlaylist,
    TrackStats as ProtoTrackStats, UpdateSmartPlaylistRequest, UpdateSmartPlaylistResponse,
};
use rockbox_playlists::rules::RuleCriteria;
use sqlx::{Pool, Sqlite};

use crate::rockbox_url;

pub struct SmartPlaylistRpc {
    store: rockbox_playlists::PlaylistStore,
    pool: Pool<Sqlite>,
    client: reqwest::Client,
}

impl SmartPlaylistRpc {
    pub fn new(
        store: rockbox_playlists::PlaylistStore,
        pool: Pool<Sqlite>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            store,
            pool,
            client,
        }
    }
}

fn criteria_to_rules(c: &ProtoRuleCriteria) -> RuleCriteria {
    use rockbox_playlists::rules::{
        Condition, MatchType, RuleField, RuleOperator, SortOrder, TimeUnit,
    };

    let match_type = if c.match_type == "any" {
        MatchType::Any
    } else {
        MatchType::All
    };

    let conditions = c
        .conditions
        .iter()
        .map(|cond| {
            let field = match cond.field.as_str() {
                "play_count" => RuleField::PlayCount,
                "skip_count" => RuleField::SkipCount,
                "last_played" => RuleField::LastPlayed,
                "last_skipped" => RuleField::LastSkipped,
                "date_added" => RuleField::DateAdded,
                "year" => RuleField::Year,
                "genre" => RuleField::Genre,
                "artist" => RuleField::Artist,
                "album" => RuleField::Album,
                "duration_ms" => RuleField::DurationMs,
                "bitrate" => RuleField::Bitrate,
                "is_liked" => RuleField::IsLiked,
                _ => RuleField::PlayCount,
            };
            let operator = match cond.operator.as_str() {
                "is" => RuleOperator::Is,
                "is_not" => RuleOperator::IsNot,
                "contains" => RuleOperator::Contains,
                "not_contains" => RuleOperator::NotContains,
                "greater_than" => RuleOperator::GreaterThan,
                "less_than" => RuleOperator::LessThan,
                "greater_than_or_equal" => RuleOperator::GreaterThanOrEqual,
                "less_than_or_equal" => RuleOperator::LessThanOrEqual,
                "equals" => RuleOperator::Equals,
                "between" => RuleOperator::Between,
                "in_last" => RuleOperator::InLast,
                "not_in_last" => RuleOperator::NotInLast,
                "is_empty" => RuleOperator::IsEmpty,
                "is_not_empty" => RuleOperator::IsNotEmpty,
                _ => RuleOperator::Is,
            };
            let unit = cond.unit.as_ref().and_then(|u| match u.as_str() {
                "days" => Some(TimeUnit::Days),
                "weeks" => Some(TimeUnit::Weeks),
                "months" => Some(TimeUnit::Months),
                "years" => Some(TimeUnit::Years),
                _ => None,
            });
            let value = cond
                .value
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone()));
            let value2 = cond
                .value2
                .as_ref()
                .map(|v| serde_json::Value::String(v.clone()));
            Condition {
                field,
                operator,
                value,
                value2,
                unit,
            }
        })
        .collect();

    let sort_by = c.sort_by.as_ref().and_then(|s| {
        use rockbox_playlists::rules::SortField;
        match s.as_str() {
            "random" => Some(SortField::Random),
            "play_count" => Some(SortField::PlayCount),
            "skip_count" => Some(SortField::SkipCount),
            "last_played" => Some(SortField::LastPlayed),
            "date_added" => Some(SortField::DateAdded),
            "year" => Some(SortField::Year),
            "title" => Some(SortField::Title),
            "artist" => Some(SortField::Artist),
            "album" => Some(SortField::Album),
            "duration_ms" => Some(SortField::DurationMs),
            _ => None,
        }
    });

    let sort_order = c.sort_order.as_ref().and_then(|s| match s.as_str() {
        "ASC" => Some(SortOrder::Asc),
        "DESC" => Some(SortOrder::Desc),
        _ => None,
    });

    RuleCriteria {
        match_type,
        conditions,
        limit: c.limit.map(|l| l as usize),
        sort_by,
        sort_order,
        rsql: c.rsql.clone().filter(|e| !e.trim().is_empty()),
    }
}

fn to_proto_criteria(p: &rockbox_playlists::rules::RuleCriteria) -> ProtoRuleCriteria {
    use rockbox_playlists::rules::{RuleField, RuleOperator, SortField, SortOrder, TimeUnit};

    let match_type = match p.match_type {
        rockbox_playlists::rules::MatchType::All => "all".to_string(),
        rockbox_playlists::rules::MatchType::Any => "any".to_string(),
    };

    let conditions: Vec<ProtoRuleCondition> = p
        .conditions
        .iter()
        .map(|c| {
            let field = match c.field {
                RuleField::PlayCount => "play_count",
                RuleField::SkipCount => "skip_count",
                RuleField::LastPlayed => "last_played",
                RuleField::LastSkipped => "last_skipped",
                RuleField::DateAdded => "date_added",
                RuleField::Year => "year",
                RuleField::Genre => "genre",
                RuleField::Artist => "artist",
                RuleField::Album => "album",
                RuleField::DurationMs => "duration_ms",
                RuleField::Bitrate => "bitrate",
                RuleField::IsLiked => "is_liked",
            };
            let operator = match c.operator {
                RuleOperator::Is => "is",
                RuleOperator::IsNot => "is_not",
                RuleOperator::Contains => "contains",
                RuleOperator::NotContains => "not_contains",
                RuleOperator::GreaterThan => "greater_than",
                RuleOperator::LessThan => "less_than",
                RuleOperator::GreaterThanOrEqual => "greater_than_or_equal",
                RuleOperator::LessThanOrEqual => "less_than_or_equal",
                RuleOperator::Equals => "equals",
                RuleOperator::Between => "between",
                RuleOperator::InLast => "in_last",
                RuleOperator::NotInLast => "not_in_last",
                RuleOperator::IsEmpty => "is_empty",
                RuleOperator::IsNotEmpty => "is_not_empty",
            };
            let unit = c.unit.as_ref().map(|u| {
                match u {
                    TimeUnit::Days => "days",
                    TimeUnit::Weeks => "weeks",
                    TimeUnit::Months => "months",
                    TimeUnit::Years => "years",
                }
                .to_string()
            });
            ProtoRuleCondition {
                field: field.to_string(),
                operator: operator.to_string(),
                value: c.value.as_ref().map(|v| v.to_string()),
                value2: c.value2.as_ref().map(|v| v.to_string()),
                unit,
            }
        })
        .collect();

    let sort_by = p.sort_by.as_ref().map(|s| {
        match s {
            SortField::Random => "random",
            SortField::PlayCount => "play_count",
            SortField::SkipCount => "skip_count",
            SortField::LastPlayed => "last_played",
            SortField::DateAdded => "date_added",
            SortField::Year => "year",
            SortField::Title => "title",
            SortField::Artist => "artist",
            SortField::Album => "album",
            SortField::DurationMs => "duration_ms",
        }
        .to_string()
    });
    let sort_order = p.sort_order.as_ref().map(|s| {
        match s {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
        .to_string()
    });

    ProtoRuleCriteria {
        match_type,
        conditions,
        limit: p.limit.map(|l| l as i32),
        sort_by,
        sort_order,
        rsql: p.rsql.clone(),
    }
}

fn to_proto_smart_playlist(p: SmartPlaylist, track_count: i64) -> ProtoSmartPlaylist {
    ProtoSmartPlaylist {
        id: p.id,
        name: p.name,
        description: p.description,
        image: p.image,
        folder_id: p.folder_id,
        is_system: p.is_system,
        rules: Some(to_proto_criteria(&p.rules)),
        created_at: p.created_at,
        updated_at: p.updated_at,
        track_count,
    }
}

fn to_proto_stats(s: TrackStats) -> ProtoTrackStats {
    ProtoTrackStats {
        track_id: s.track_id,
        play_count: s.play_count,
        skip_count: s.skip_count,
        last_played: s.last_played,
        last_skipped: s.last_skipped,
        updated_at: s.updated_at,
    }
}

#[tonic::async_trait]
impl SmartPlaylistService for SmartPlaylistRpc {
    async fn get_smart_playlists(
        &self,
        _request: tonic::Request<GetSmartPlaylistsRequest>,
    ) -> Result<tonic::Response<GetSmartPlaylistsResponse>, tonic::Status> {
        let playlists = self
            .store
            .list_smart_playlists()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let (candidates, _) = resolver::build_candidates(&self.store, &self.pool)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let mut out = Vec::with_capacity(playlists.len());
        for p in playlists {
            let track_count =
                rockbox_playlists::rules::resolve(&p.rules, candidates.clone()).len() as i64;
            out.push(to_proto_smart_playlist(p, track_count));
        }
        Ok(tonic::Response::new(GetSmartPlaylistsResponse {
            playlists: out,
        }))
    }

    async fn get_smart_playlist(
        &self,
        request: tonic::Request<GetSmartPlaylistRequest>,
    ) -> Result<tonic::Response<GetSmartPlaylistResponse>, tonic::Status> {
        let id = request.into_inner().id;
        let playlist = self
            .store
            .get_smart_playlist(&id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let proto = match playlist {
            Some(p) => {
                let track_count = resolver::count_tracks(&self.store, &self.pool, &p.rules)
                    .await
                    .map_err(|e| tonic::Status::internal(e.to_string()))?;
                Some(to_proto_smart_playlist(p, track_count))
            }
            None => None,
        };
        Ok(tonic::Response::new(GetSmartPlaylistResponse {
            playlist: proto,
        }))
    }

    async fn create_smart_playlist(
        &self,
        request: tonic::Request<CreateSmartPlaylistRequest>,
    ) -> Result<tonic::Response<CreateSmartPlaylistResponse>, tonic::Status> {
        let req = request.into_inner();
        let rules = req
            .rules
            .as_ref()
            .map(criteria_to_rules)
            .unwrap_or_default();
        let playlist = self
            .store
            .create_smart_playlist(
                &req.name,
                req.description.as_deref(),
                req.image.as_deref(),
                req.folder_id.as_deref(),
                &rules,
            )
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let track_count = resolver::count_tracks(&self.store, &self.pool, &playlist.rules)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        #[cfg(not(feature = "fts5"))]
        {
            let ts_p = TsPlaylist {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                description: playlist.description.clone(),
                image: playlist.image.clone(),
                is_smart: true,
                track_count,
            };
            let _ = insert_playlists(vec![ts_p]).await;
        }
        Ok(tonic::Response::new(CreateSmartPlaylistResponse {
            playlist: Some(to_proto_smart_playlist(playlist, track_count)),
        }))
    }

    async fn update_smart_playlist(
        &self,
        request: tonic::Request<UpdateSmartPlaylistRequest>,
    ) -> Result<tonic::Response<UpdateSmartPlaylistResponse>, tonic::Status> {
        let req = request.into_inner();
        let rules = req
            .rules
            .as_ref()
            .map(criteria_to_rules)
            .unwrap_or_default();
        self.store
            .update_smart_playlist(
                &req.id,
                &req.name,
                req.description.as_deref(),
                req.image.as_deref(),
                req.folder_id.as_deref(),
                &rules,
            )
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        #[cfg(not(feature = "fts5"))]
        if let Ok(Some(updated)) = self.store.get_smart_playlist(&req.id).await {
            let track_count = resolver::count_tracks(&self.store, &self.pool, &updated.rules)
                .await
                .unwrap_or(0);
            let ts_p = TsPlaylist {
                id: updated.id.clone(),
                name: updated.name.clone(),
                description: updated.description.clone(),
                image: updated.image.clone(),
                is_smart: true,
                track_count,
            };
            let _ = insert_playlists(vec![ts_p]).await;
        }
        Ok(tonic::Response::new(UpdateSmartPlaylistResponse {}))
    }

    async fn delete_smart_playlist(
        &self,
        request: tonic::Request<DeleteSmartPlaylistRequest>,
    ) -> Result<tonic::Response<DeleteSmartPlaylistResponse>, tonic::Status> {
        let id = request.into_inner().id;
        self.store
            .delete_smart_playlist(&id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        #[cfg(not(feature = "fts5"))]
        let _ = ts_delete_playlist(&id).await;
        Ok(tonic::Response::new(DeleteSmartPlaylistResponse {}))
    }

    async fn get_smart_playlist_tracks(
        &self,
        request: tonic::Request<GetSmartPlaylistTracksRequest>,
    ) -> Result<tonic::Response<GetSmartPlaylistTracksResponse>, tonic::Status> {
        let id = request.into_inner().id;

        let criteria = match self
            .store
            .get_smart_playlist(&id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
        {
            Some(p) => p.rules,
            None => {
                return Ok(tonic::Response::new(GetSmartPlaylistTracksResponse {
                    track_ids: vec![],
                }))
            }
        };

        let tracks = resolver::resolve_tracks(&self.store, &self.pool, &criteria)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let track_ids = tracks.into_iter().map(|t| t.id).collect();

        Ok(tonic::Response::new(GetSmartPlaylistTracksResponse {
            track_ids,
        }))
    }

    async fn play_smart_playlist(
        &self,
        request: tonic::Request<PlaySmartPlaylistRequest>,
    ) -> Result<tonic::Response<PlaySmartPlaylistResponse>, tonic::Status> {
        let id = request.into_inner().id;
        let url = format!("{}/smart-playlists/{}/play", rockbox_url(), id);
        self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(PlaySmartPlaylistResponse {}))
    }

    async fn record_track_played(
        &self,
        request: tonic::Request<RecordTrackPlayedRequest>,
    ) -> Result<tonic::Response<RecordTrackPlayedResponse>, tonic::Status> {
        let track_id = request.into_inner().track_id;
        self.store
            .record_play(&track_id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(RecordTrackPlayedResponse {}))
    }

    async fn record_track_skipped(
        &self,
        request: tonic::Request<RecordTrackSkippedRequest>,
    ) -> Result<tonic::Response<RecordTrackSkippedResponse>, tonic::Status> {
        let track_id = request.into_inner().track_id;
        self.store
            .record_skip(&track_id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(RecordTrackSkippedResponse {}))
    }

    async fn get_track_stats(
        &self,
        request: tonic::Request<GetTrackStatsRequest>,
    ) -> Result<tonic::Response<GetTrackStatsResponse>, tonic::Status> {
        let track_id = request.into_inner().track_id;
        let stats = self
            .store
            .get_track_stats(&track_id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetTrackStatsResponse {
            stats: stats.map(to_proto_stats),
        }))
    }
}
