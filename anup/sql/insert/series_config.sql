REPLACE INTO SeriesConfig (id, nickname, path, episode_matcher, player_args)
VALUES
    (
        :id,
        :nickname,
        :path,
        :episode_matcher,
        :player_args
    )