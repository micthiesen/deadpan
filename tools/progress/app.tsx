// Deadpan progress dashboard, published as an artifact on artifacts.boo.
// data/progress.json is kept in sync with docs/progress.json on GitHub main by the
// artifact's live data source; this file only renders it. See docs/PROGRESS.md.
import { useState } from "react";
import {
  Badge,
  Callout,
  Chart,
  Link,
  Page,
  Progress,
  Section,
  Stat,
  Stats,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@a/ui";
import { useData } from "@a/hooks";

type Stage = "planned" | "partial" | "mostly" | "done";
type Feature = { id: string; name: string; status: Stage; requirements: string[]; note: string };
type Point = {
  date: string;
  percent: number;
  complete_sections: number;
  revision: string;
  retrospective?: boolean;
  summary: string;
};
type Report = {
  schema_version: number;
  updated_at: string;
  estimate_note: string;
  focus: string;
  features: Feature[];
  history: Point[];
};

const STAGES: Record<Stage, { label: string; tone: "positive" | "primary" | "caution" | "neutral" }> = {
  done: { label: "Done & verified", tone: "positive" },
  mostly: { label: "Mostly built", tone: "primary" },
  partial: { label: "Partly built", tone: "caution" },
  planned: { label: "Planned", tone: "neutral" },
};
const ORDER: Stage[] = ["partial", "mostly", "planned", "done"];
const REPO = "https://github.com/micthiesen/deadpan";

const day = (value: string) =>
  new Date(`${value}T12:00:00Z`).toLocaleDateString(undefined, { month: "short", day: "numeric" });
const signed = (value: number) => `${value >= 0 ? "+" : ""}${value.toFixed(1).replace(/\.0$/, "")}`;

export default function App() {
  const report = useData<Report>("progress.json");
  const [remainingOnly, setRemainingOnly] = useState(false);

  if (report.schema_version !== 1 || !report.history?.length) {
    return (
      <Page title="Deadpan progress">
        <Callout tone="danger" title="Unsupported progress report">
          docs/progress.json has an unexpected shape. Check tools/progress/update.py.
        </Callout>
      </Page>
    );
  }

  const points = report.history;
  const first = points[0];
  const latest = points[points.length - 1];
  const change = latest.percent - first.percent;
  const days = (Date.parse(latest.date) - Date.parse(first.date)) / 86_400_000;
  const rate = days > 0 ? change / days : null;
  const done = report.features.filter((f) => f.status === "done").length;
  const features = [...report.features]
    .filter((f) => !remainingOnly || f.status !== "done")
    .sort((a, b) => ORDER.indexOf(a.status) - ORDER.indexOf(b.status));
  const updated = new Date(report.updated_at).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });

  return (
    <Page title="Deadpan progress" subtitle={`Updated ${updated}`} wide>
      <Progress value={latest.percent} label={`Estimated spec coverage: ~${Math.round(latest.percent)}%`} />

      <Stats>
        <Stat label="Spec coverage" value={`~${Math.round(latest.percent)}%`} hint="rough estimate, ±5 points" />
        <Stat label={`Since ${day(first.date)}`} value={`${signed(change)} pts`} />
        <Stat label="Average change" value={rate === null ? "Collecting…" : `${signed(rate)} / day`} hint="calendar days, not a forecast" />
        <Stat label="Groups done & verified" value={`${done} / ${report.features.length}`} hint={`${latest.complete_sections} / 24 strict sections`} />
      </Stats>

      <Callout tone="info" title="Working now">
        {report.focus}
      </Callout>

      <Tabs defaultValue="features">
        <TabsList>
          <TabsTrigger value="features">Features</TabsTrigger>
          <TabsTrigger value="history">History</TabsTrigger>
        </TabsList>

        <TabsContent value="features">
          <div className="mb-3 flex justify-end">
            <Switch checked={remainingOnly} onCheckedChange={setRemainingOnly} label="Remaining only" />
          </div>
          <div className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
            {features.map((f) => (
              <div key={f.id} className="px-5 py-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-medium">{f.name}</span>
                  <Badge tone={STAGES[f.status]?.tone ?? "neutral"}>{STAGES[f.status]?.label ?? f.status}</Badge>
                </div>
                <p className="mt-1.5 text-sm text-muted-foreground">{f.note}</p>
                <p className="mt-2 font-mono text-xs text-muted-foreground/80">{f.requirements.join(" · ")}</p>
              </div>
            ))}
          </div>
        </TabsContent>

        <TabsContent value="history">
          <Chart
            type="area"
            data={points.map((p) => ({ day: day(p.date), coverage: p.percent }))}
            x="day"
            y="coverage"
            height={220}
          />
          <Section title="Milestones">
            <div className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
              {[...points].reverse().map((p, i) => (
                <div key={`${p.date}-${i}`} className="px-5 py-4">
                  <div className="flex items-center justify-between gap-4">
                    <span className="font-mono text-xs text-muted-foreground">
                      {day(p.date)}
                      {p.retrospective ? " · retrospective baseline" : ""}
                    </span>
                    <span className="text-sm font-semibold tabular-nums">~{Math.round(p.percent)}%</span>
                  </div>
                  <p className="mt-1.5 text-sm">{p.summary}</p>
                  <Link href={`${REPO}/commit/${p.revision}`}>
                    <span className="font-mono text-xs">{p.revision.slice(0, 10)}</span>
                  </Link>
                </div>
              ))}
            </div>
          </Section>
        </TabsContent>
      </Tabs>

      <p className="mt-8 text-xs text-muted-foreground">
        {report.estimate_note}{" "}
        <Link href={`${REPO}/blob/main/docs/REQUIREMENTS.md`}>Evidence and remaining work</Link>
      </p>
    </Page>
  );
}
