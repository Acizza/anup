SELECT
    id,
    nickname,
    path,
    episode_matcher,
    player_args
FROM
    SeriesConfig
WHERE
    nickname = ?1