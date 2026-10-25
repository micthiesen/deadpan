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
  assessment_id?: string;
  summary: string;
};
type Report = {
  schema_version: number;
  updated_at: string;
  estimate_note: string;
  focus: string;
  features: Feature[];
  history: Point[];
  assessment?: {
    id: string;
    requirements: { id: string; name: string; criteria: { score: number }[] }[];
    gates: { id: string; tracker_status: string; local_blockers: string[] }[];
    effects: { complete: number; total: number };
  };
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

export default function App() {
  const report = useData<Report>("progress.json");
  const [remainingOnly, setRemainingOnly] = useState(false);

  if (report.schema_version !== 1 || !report.history?.length || !report.assessment) {
    return (
      <Page title="Deadpan progress">
        <Callout tone="danger" title="Unsupported progress report">
          docs/progress.json has an unexpected shape. Check tools/progress/update.py.
        </Callout>
      </Page>
    );
  }

  const points = report.history;
  const latest = points[points.length - 1];
  const assessment = report.assessment;
  const comparable = points.filter((p) => p.assessment_id === assessment.id);
  const closedGates = assessment.gates.filter((g) => g.tracker_status === "Complete").length;
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
      <Progress value={latest.percent} label={`Assessed requirement coverage: ~${Math.round(latest.percent)}%`} />

      <Stats>
        <Stat label="Coverage score" value={`~${Math.round(latest.percent)}%`} hint="acceptance scope, not time remaining" />
        <Stat label="Requirements complete" value={`${latest.complete_sections} / 24`} hint="strict completion" />
        <Stat label="Delivery gates closed" value={`${closedGates} / ${assessment.gates.length}`} hint="release readiness remains open" />
        <Stat label="Picture effect families" value={`${assessment.effects.complete} / ${assessment.effects.total}`} hint="implemented and verified" />
      </Stats>

      <Callout tone="info" title="Working now">
        {report.focus}
      </Callout>

      <Tabs defaultValue="coverage">
        <TabsList>
          <TabsTrigger value="coverage">Coverage</TabsTrigger>
          <TabsTrigger value="features">Features</TabsTrigger>
          <TabsTrigger value="history">History</TabsTrigger>
        </TabsList>

        <TabsContent value="coverage">
          <Callout tone="caution" title="Recalibrated October 25">
            The old 89% was an uncalibrated estimate carried through 137 updates since October 8.
            The current score uses fixed acceptance criteria: complete = 1, partial = ½, absent = 0.
            Each of the 24 requirements has equal weight. This is not a release or completion-time forecast.
            {" "}<Link href={`${REPO}/blob/main/docs/PROGRESS_ASSESSMENT.md`}>Read the audit and remaining work</Link>
          </Callout>
          <Section title="Requirement coverage">
            <div className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
              {assessment.requirements.map((r) => {
                const score = 100 * r.criteria.reduce((sum, c) => sum + c.score, 0) / r.criteria.length;
                const complete = r.criteria.filter((c) => c.score === 1).length;
                const partial = r.criteria.filter((c) => c.score === 0.5).length;
                return <div key={r.id} className="px-5 py-3">
                  <div className="flex items-start justify-between gap-4">
                    <span><span className="font-mono text-xs">{r.id}</span> {r.name}</span>
                    <span className="font-semibold tabular-nums">{Math.round(score)}%</span>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">{complete} complete · {partial} partial · {r.criteria.length - complete - partial} absent</p>
                </div>;
              })}
            </div>
          </Section>
          <Section title="Delivery gates">
            <div className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
              {assessment.gates.map((g) => <div key={g.id} className="px-5 py-3">
                <p className="font-medium">Gate {g.id}: {g.tracker_status}</p>
                <p className="mt-1 text-sm text-muted-foreground">{g.local_blockers.join("; ")}</p>
              </div>)}
            </div>
          </Section>
        </TabsContent>

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
          <p className="mb-3 text-sm text-muted-foreground">Earlier estimates are preserved below. The October 25 recalibration changes the method; its drop does not represent lost implementation. Earlier values cannot establish a progress rate.</p>
          {comparable.length > 1 && <Chart
            type="area"
            data={comparable.map((p) => ({ day: day(p.date), coverage: p.percent }))}
            x="day"
            y="coverage"
            height={220}
          />}
          <Section title="Milestones">
            <div className="divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
              {[...points].reverse().map((p, i) => (
                <div key={`${p.date}-${i}`} className="px-5 py-4">
                  <div className="flex items-center justify-between gap-4">
                    <span className="font-mono text-xs text-muted-foreground">
                      {day(p.date)}
                      {p.retrospective ? " · retrospective baseline" : ""}
                      {!p.assessment_id ? " · legacy estimate" : " · acceptance ledger"}
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
