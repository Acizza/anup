REPLACE INTO AnimeInfo (
    id,
    title_preferred,
    title_romaji,
    episodes,
    episode_length_mins,
    sequel
)
VALUES
    (
        :id,
        :title_preferred,
        :title_romaji,
        :episodes,
        :episode_length,
        :sequel
    )