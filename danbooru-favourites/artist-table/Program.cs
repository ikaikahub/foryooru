// Builds the artist neighbour table from favourites.sqlite: for each artist, the artists their fans also like.
// Writes out/artist_neighbours.bin for the app and prints a few examples. ARTISTS.md explains the scoring.
// Run from danbooru-favourites: dotnet run -c Release --project artist-table
using System.Diagnostics;
using System.Text;
using Microsoft.Data.Sqlite;

const string DbPath = "favourites.sqlite";
const string OutPath = "out/artist_neighbours.bin";
// artists with fewer fans have too little overlap to say who they're like
const int MinFans = 3;
// 0.5 is plain cosine: popular artists are compared fairly, not pushed down
const double PopularityDamping = 0.5;
// users who are fans of many artists count less, so one person who favourites everything doesn't link everything
const double HeavyUserDamping = 0.5;
// a link resting on few shared fans keeps shared / (shared + Shrink) of its score
const double Shrink = 5.0;
// neighbours kept per artist, and the fewest shared fans a neighbour needs
const int Neighbours = 20;
const int MinShared = 2;
string[] examples = ["nyantcha", "shefu", "kusakihara_toshiyuki", "ikura_(downdexp)"];

if (!File.Exists(DbPath))
{
  Console.Error.WriteLine($"{DbPath} not found; run get_favourites.py from danbooru-favourites first");
  return 1;
}
var clock = Stopwatch.StartNew();

// who is a fan of whom: a user is a fan of an artist after favouriting at least one of their posts
var artistsOfUser = new Dictionary<long, HashSet<string>>();
using (var db = new SqliteConnection($"Data Source={DbPath};Mode=ReadOnly"))
{
  db.Open();
  using var command = db.CreateCommand();
  command.CommandText = "SELECT favourites.user_id, posts.artists FROM favourites JOIN posts ON posts.id = favourites.post_id";
  using var reader = command.ExecuteReader();
  while (reader.Read())
  {
    var user = reader.GetInt64(0);
    if (!artistsOfUser.TryGetValue(user, out var artists))
    {
      artists = [];
      artistsOfUser[user] = artists;
    }
    // Danbooru files voice actors of animated posts under artist tags
    foreach (var name in reader.GetString(1).Split(' ', StringSplitOptions.RemoveEmptyEntries))
    {
      if (!name.EndsWith("_(voice_actor)")) artists.Add(name);
    }
  }
}

var fanCount = new Dictionary<string, int>();
foreach (var artists in artistsOfUser.Values)
{
  foreach (var artist in artists) fanCount[artist] = fanCount.GetValueOrDefault(artist) + 1;
}
var names = fanCount.Where(pair => pair.Value >= MinFans).Select(pair => pair.Key).Order(StringComparer.Ordinal).ToArray();
var indexOf = names.Select((name, index) => (name, index)).ToDictionary(pair => pair.name, pair => pair.index);

var usersArtists = artistsOfUser.Values
  .Select(artists => artists.Where(indexOf.ContainsKey).Select(artist => indexOf[artist]).ToArray())
  .ToArray();
var userWeight = usersArtists.Select(artists => 1.0 / Math.Pow(Math.Max(artists.Length, 1), HeavyUserDamping)).ToArray();
var fansOf = new List<int>[names.Length];
for (var artist = 0; artist < names.Length; artist++) fansOf[artist] = [];
for (var user = 0; user < usersArtists.Length; user++)
{
  foreach (var artist in usersArtists[user]) fansOf[artist].Add(user);
}
var fans = fansOf.Select(list => list.Count).ToArray();
Console.WriteLine($"{usersArtists.Length:N0} users, {names.Length:N0} artists with {MinFans}+ fans ({clock.Elapsed.TotalSeconds:F0}s)");

var best = new int[names.Length * Neighbours];
var bestScores = new byte[names.Length * Neighbours];
Array.Fill(best, -1);
var done = 0;
Parallel.For(
  0,
  names.Length,
  // each thread keeps its own tallies, cleared after each artist through the list of entries it touched
  () => (shared: new int[names.Length], linked: new double[names.Length], touched: new List<int>()),
  (artist, _, tallies) =>
  {
    var (shared, linked, touched) = tallies;
    foreach (var user in fansOf[artist])
    {
      foreach (var other in usersArtists[user])
      {
        if (shared[other] == 0) touched.Add(other);
        shared[other]++;
        linked[other] += userWeight[user];
      }
    }
    var candidates = new List<(int artist, double score)>();
    foreach (var other in touched)
    {
      if (other != artist && shared[other] >= MinShared)
      {
        var score = linked[other] / (Math.Pow(fans[other], PopularityDamping) * Math.Sqrt(fans[artist]));
        candidates.Add((other, score * shared[other] / (shared[other] + Shrink)));
      }
      shared[other] = 0;
      linked[other] = 0;
    }
    touched.Clear();
    candidates.Sort((a, b) => b.score.CompareTo(a.score));
    var peak = candidates.Count > 0 ? candidates[0].score : 1;
    for (var slot = 0; slot < Math.Min(Neighbours, candidates.Count); slot++)
    {
      best[artist * Neighbours + slot] = candidates[slot].artist;
      bestScores[artist * Neighbours + slot] = (byte)Math.Round(candidates[slot].score / peak * 255);
    }
    var finished = Interlocked.Increment(ref done);
    if (finished % 20_000 == 0) Console.WriteLine($"  {finished:N0} of {names.Length:N0} artists, {clock.Elapsed.TotalSeconds:F0}s");
    return tallies;
  },
  _ => { }
);

// FYAN, artist count and neighbours per artist, each name (u16 byte length then UTF-8), then per artist its
// neighbour indexes (i32, -1 where there are fewer) and scores as bytes, 255 for its best neighbour. little-endian
Directory.CreateDirectory(Path.GetDirectoryName(OutPath)!);
using (var output = new BinaryWriter(File.Create(OutPath)))
{
  output.Write("FYAN"u8);
  output.Write((uint)names.Length);
  output.Write((uint)Neighbours);
  foreach (var name in names)
  {
    var encoded = Encoding.UTF8.GetBytes(name);
    output.Write((ushort)encoded.Length);
    output.Write(encoded);
  }
  foreach (var index in best) output.Write(index);
  output.Write(bestScores);
}
var withNeighbours = Enumerable.Range(0, names.Length).Count(artist => best[artist * Neighbours] >= 0);
Console.WriteLine($"wrote {OutPath} ({new FileInfo(OutPath).Length / 1_000_000.0:F1} MB), {withNeighbours:N0} artists have neighbours, {clock.Elapsed.TotalSeconds:F0}s in all");

foreach (var example in examples)
{
  if (!indexOf.TryGetValue(example, out var artist)) continue;
  var shown = Enumerable.Range(0, 12).Select(slot => best[artist * Neighbours + slot]).Where(index => index >= 0).Select(index => $"{names[index]} ({fans[index]})");
  Console.WriteLine($"\n{example} ({fans[artist]} fans): {string.Join(", ", shown)}");
}
return 0;
