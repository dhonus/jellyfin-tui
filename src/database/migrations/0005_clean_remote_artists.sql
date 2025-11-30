DELETE FROM artists
WHERE id NOT IN (
    SELECT id FROM artists
    WHERE json_extract(artist, '$.UserData.Key') LIKE 'Artist-%'
);
