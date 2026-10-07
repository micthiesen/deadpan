const config = { maxHeight: 900 };

export function App() {
  const query = useQuery(tools.github_com.get_file_contents.queryOptions({
    owner: "micthiesen", repo: "deadpan", path: "docs/progress.json", ref: "refs/heads/main"
  }, { staleTime: 60000 }));
  const [tab, setTab] = useState("features");
  const [remaining, setRemaining] = useState(false);
  if (query.isLoading) return <ArtifactLoading variant="list" rows={6} />;
  if (query.error) return <ArtifactError error={query.error} onRetry={query.refetch} />;
  if (!query.data?.ok) return <ArtifactError error={new Error(query.data?.error?.message || "Couldn't read the progress report.")} onRetry={query.refetch} />;
  const body = query.data.data?.content?.find(item => item.type === "resource" && typeof item.resource?.text === "string")?.resource?.text;
  let report;
  try {
    report = JSON.parse(body || "");
    if (report.schema_version !== 1 || !report.history?.length || !Array.isArray(report.features)) throw new Error("Unsupported progress report.");
  } catch (error) {
    return <ArtifactError error={error} onRetry={query.refetch} />;
  }
  const points = report.history;
  const latest = points[points.length - 1];
  const first = points[0];
  const change = latest.percent - first.percent;
  const days = (Date.parse(latest.date) - Date.parse(first.date)) / 86400000;
  const rate = days > 0 ? change / days : null;
  const stages = { done: "Done & verified", mostly: "Mostly built", partial: "Partly built", planned: "Planned" };
  const dateLabel = value => new Date(value + "T12:00:00Z").toLocaleDateString(undefined, { month: "short", day: "numeric" });
  const signed = value => `${value >= 0 ? "+" : ""}${value.toFixed(1).replace(/\.0$/, "")}`;
  const rows = report.features.filter(feature => !remaining || feature.status !== "done");
  const done = report.features.filter(feature => feature.status === "done").length;
  return <div className="flex h-full flex-col gap-4 text-foreground">
    <header className="flex shrink-0 items-start justify-between gap-4">
      <div>
        <h2 className="text-xl font-semibold tracking-tight">Deadpan progress</h2>
        <p className="mt-1 text-xs text-muted-foreground">Updated {new Date(report.updated_at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}</p>
      </div>
      <Button variant="outline" size="sm" onClick={() => query.refetch()}>{query.isFetching ? "Refreshing…" : "Refresh"}</Button>
    </header>
    <section className="shrink-0 space-y-3">
      <div className="flex items-end justify-between gap-4">
        <span className="font-mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">Estimated spec coverage</span>
        <span className="text-2xl font-semibold tabular-nums">~{Math.round(latest.percent)}%</span>
      </div>
      <div role="progressbar" aria-label="Estimated specification coverage" aria-valuemin={0} aria-valuemax={100} aria-valuenow={latest.percent} className="h-14 overflow-hidden rounded-lg bg-muted">
        <div className="h-full rounded-lg bg-foreground" style={{ width: `${latest.percent}%` }} />
      </div>
      <div className="grid grid-cols-3 gap-4 border-b border-border pb-4">
        <div><div className="text-lg font-medium tabular-nums">{signed(change)} points</div><p className="text-xs text-muted-foreground">Since resuming {dateLabel(first.date)}</p></div>
        <div><div className="text-lg font-medium tabular-nums">{rate === null ? "Collecting…" : `~${signed(rate)} / day`}</div><p className="text-xs text-muted-foreground">Average estimated change</p></div>
        <div><div className="text-lg font-medium tabular-nums">{done} / {report.features.length}</div><p className="text-xs text-muted-foreground">Feature groups done & verified</p></div>
      </div>
      <p className="text-sm"><span className="font-medium">Working now: </span>{report.focus}</p>
    </section>
    <div className="flex shrink-0 items-center justify-between gap-2">
      <Tabs value={tab} onValueChange={setTab}><TabsList><TabsTrigger value="features">Features</TabsTrigger><TabsTrigger value="history">Progress History</TabsTrigger></TabsList></Tabs>
      {tab === "features" && <Button variant="ghost" size="sm" onClick={() => setRemaining(!remaining)}>{remaining ? "Show All" : "Remaining Only"}</Button>}
    </div>
    <div className="min-h-0 flex-1 overflow-auto">
      {tab === "features" ? <div className="divide-y divide-border rounded-lg border border-border">
        {rows.map(feature => <div key={feature.id} className="px-4 py-3">
          <div className="flex flex-wrap items-center justify-between gap-2"><span className="text-sm font-medium">{feature.name}</span><Badge variant={feature.status === "done" ? "secondary" : "outline"}>{stages[feature.status] || feature.status}</Badge></div>
          <p className="mt-1 text-xs text-muted-foreground">{feature.note}</p>
        </div>)}
      </div> : <div className="space-y-4">
        <ChartContainer config={{ percent: { label: "Estimated coverage", color: "var(--chart-1)" } }} className="h-[180px] w-full">
          <LineChart data={points.map((point, index) => ({ ...point, milestone: index, day: dateLabel(point.date) }))}>
            <CartesianGrid vertical={false} stroke="var(--border)" />
            <XAxis dataKey="day" tickLine={false} axisLine={false} tick={{ fill: "var(--muted-foreground)", fontSize: 11 }} />
            <YAxis domain={[0, 100]} tickFormatter={value => `${Math.round(value)}%`} tickLine={false} axisLine={false} tick={{ fill: "var(--muted-foreground)", fontSize: 11 }} width={40} />
            <ChartTooltip content={<ChartTooltipContent />} />
            <Line dataKey="percent" stroke="var(--chart-1)" strokeWidth={2} dot={false} strokeLinecap="round" />
          </LineChart>
        </ChartContainer>
        <div className="divide-y divide-border rounded-lg border border-border">
          {[...points].reverse().map((point, index) => <div key={`${point.date}-${index}`} className="px-4 py-3">
            <div className="flex items-center justify-between gap-4"><span className="font-mono text-xs text-muted-foreground">{dateLabel(point.date)}{point.retrospective ? " · retrospective baseline" : ""}</span><span className="text-sm font-medium">~{Math.round(point.percent)}%</span></div>
            <p className="mt-1 text-sm">{point.summary}</p>
            <a className="mt-1 inline-block font-mono text-xs text-muted-foreground underline" href={`https://github.com/micthiesen/deadpan/commit/${point.revision}`} target="_blank" rel="noreferrer">{point.revision}</a>
          </div>)}
        </div>
      </div>}
    </div>
    <footer className="shrink-0 space-y-1 border-t border-border pt-3 text-xs text-muted-foreground">
      <p>{report.estimate_note} The rate uses calendar days and is not a forecast.</p>
      <p>{latest.complete_sections} / 24 strict requirement sections complete. <a className="underline" href="https://github.com/micthiesen/deadpan/blob/main/docs/REQUIREMENTS.md" target="_blank" rel="noreferrer">Evidence and remaining work</a></p>
    </footer>
  </div>;
}
