SELECT
    id,
    title_preferred,
    title_romaji,
    episodes,
    episode_length_mins,
    sequel
FROM
    AnimeInfo
WHERE
    id = ?1