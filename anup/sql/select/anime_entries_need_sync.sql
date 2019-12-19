SELECT
    id,
    watched_episodes,
    score,
    status,
    times_rewatched,
    start_date,
    finish_date,
    needs_sync
FROM
    AnimeEntry
WHERE
    needs_sync = 1