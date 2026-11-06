'use client';

import {
  Fragment,
  type HTMLAttributes,
  type ReactNode,
  useEffect,
  useState,
} from 'react';
import { ArrowRight, BellRing, BookOpen, Database, Search, Shield, Terminal, Webhook } from 'lucide-react';
import { cva } from 'class-variance-authority';
import { cn } from '@/lib/cn';

const previewButtonVariants = cva('h-8 w-24 rounded-full text-sm font-medium transition-colors', {
  variants: {
    active: {
      true: 'text-fd-primary-foreground',
      false: 'text-fd-muted-foreground',
    },
  },
});

export function Hero() {
  return (
    <>
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_top_left,rgba(255,168,97,0.24),transparent_40%),radial-gradient(circle_at_80%_20%,rgba(198,187,88,0.22),transparent_35%)] dark:bg-[radial-gradient(circle_at_top_left,rgba(255,243,131,0.14),transparent_40%),radial-gradient(circle_at_80%_20%,rgba(252,119,68,0.18),transparent_35%)]" />
      <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-fd-background/30" />
      <div className="absolute left-[23%] top-[410px] z-1 hidden w-[min(1100px,78vw)] rounded-2xl border border-white/60 bg-fd-card/90 p-3 shadow-2xl shadow-black/15 backdrop-blur lg:block">
        <HeroPreviewPanel />
      </div>
    </>
  );
}

export function CreateWorkflowAnimation(props: HTMLAttributes<HTMLDivElement>) {
  const command = 'openkakao-cli local-send "Me" "hi"';
  const tickTime = 100;
  const timeCommandEnter = command.length;
  const timeCommandRun = timeCommandEnter + 3;
  const timeCommandEnd = timeCommandRun + 4;
  const timeEnd = timeCommandEnd + 1;
  const [tick, setTick] = useState(timeEnd);

  useEffect(() => {
    const timer = setInterval(() => {
      setTick((prev) => (prev >= timeEnd ? prev : prev + 1));
    }, tickTime);

    return () => clearInterval(timer);
  }, [timeEnd]);

  return (
    <div
      {...props}
      onMouseEnter={() => {
        if (tick >= timeEnd) setTick(0);
      }}
    >
      <pre className="min-h-[220px] font-mono text-sm">
        <code className="grid gap-1">
          <span>
            {command.substring(0, tick)}
            {tick < timeCommandEnter && (
              <span className="inline-block h-3 w-1 animate-pulse bg-fd-foreground" />
            )}
          </span>
          {tick > timeCommandRun && (
            <Fragment>
              <span className="text-fd-muted-foreground">Finding the chat through Accessibility...</span>
              {tick > timeCommandRun + 1 && (
                <span className="text-fd-muted-foreground">Inserting the message into the composer...</span>
              )}
              {tick > timeCommandRun + 2 && (
                <span className="text-fd-muted-foreground">Verifying the new bubble...</span>
              )}
              {tick > timeCommandRun + 3 && (
                <span className="font-medium text-brand">Delivered and verified in 0.4 s.</span>
              )}
            </Fragment>
          )}
        </code>
      </pre>
    </div>
  );
}

export function PreviewImages(props: HTMLAttributes<HTMLDivElement>) {
  const [active, setActive] = useState(0);
  const previews = [
    { name: 'Docs' },
    { name: 'History' },
    { name: 'Automation' },
  ];

  return (
    <div {...props} className={cn('relative grid', props.className)}>
      <div className="absolute bottom-0 left-1/2 z-2 flex -translate-x-1/2 flex-row rounded-full border bg-fd-card p-0.5 shadow-xl">
        <div
          role="none"
          className="absolute z-[-1] h-8 w-24 rounded-full bg-fd-primary transition-transform"
          style={{ transform: `translateX(calc(var(--spacing) * 24 * ${active}))` }}
        />
        {previews.map((item, i) => (
          <button
            key={item.name}
            className={cn(previewButtonVariants({ active: active === i }))}
            onClick={() => setActive(i)}
          >
            {item.name}
          </button>
        ))}
      </div>
      <div className="col-start-1 row-start-1 h-[430px] select-none rounded-2xl border bg-fd-card p-4 shadow-lg">
        <div className={cn(active === 0 ? 'animate-in slide-in-from-bottom-12 fade-in duration-800' : 'hidden')}>
          <DocsSurface />
        </div>
        <div className={cn(active === 1 ? 'animate-in slide-in-from-bottom-12 fade-in duration-800' : 'hidden')}>
          <HistorySurface />
        </div>
        <div className={cn(active === 2 ? 'animate-in slide-in-from-bottom-12 fade-in duration-800' : 'hidden')}>
          <AutomationSurface />
        </div>
      </div>
    </div>
  );
}

function HeroPreviewPanel() {
  return (
    <div className="overflow-hidden rounded-xl border bg-[#191919] text-white">
      <div className="flex items-center gap-3 border-b border-white/10 px-4 py-3">
        <div className="flex items-center gap-2 text-sm font-medium">
          <div className="size-3 rounded-full bg-[#FEE500]" />
          OpenKakao
        </div>
        <div className="flex flex-1 items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-xs text-white/60">
          <Search className="size-3.5" />
          Search docs and commands
          <div className="ml-auto rounded border border-white/10 px-1.5 py-0.5 text-[10px]">⌘K</div>
        </div>
        <div className="flex items-center gap-3 text-white/60">
          <Shield className="size-4" />
          <BookOpen className="size-4" />
          <Terminal className="size-4" />
        </div>
      </div>

      <div className="grid min-h-[420px] grid-cols-[240px_minmax(0,1fr)_220px]">
        <div className="border-r border-white/10 bg-black/15 px-4 py-4">
          <div className="mb-4 rounded-lg border border-[#FEE500]/30 bg-[#FEE500]/10 px-3 py-2 text-sm font-medium text-[#FEE500]">
            CLI
          </div>
          <SidebarGroup
            title="Introduction"
            items={['Getting Started', 'Authentication', 'Transport Boundary']}
            active="Getting Started"
          />
          <SidebarGroup
            title="Workflow Surfaces"
            items={['Read / export', 'Watch events', 'Send carefully']}
          />
          <SidebarGroup
            title="Security"
            items={['Trust model', 'Data & credentials', 'Safe usage']}
          />
        </div>

        <div className="px-5 py-5">
          <div className="mb-4 flex items-center gap-5 text-sm text-white/65">
            <span className="border-b border-[#F58B54] pb-1 text-[#F58B54]">Getting Started</span>
            <span>Read</span>
            <span>Watch</span>
            <span>Export</span>
            <span>Automation</span>
          </div>
          <h2 className="mb-2 text-4xl font-semibold tracking-tight">Getting Started</h2>
          <p className="mb-6 text-white/60">Go from local app state to a usable workflow in a few commands.</p>

          <div className="grid gap-4 md:grid-cols-2">
            <SurfaceCard
              icon={<Terminal className="size-4" />}
              title="Install and login"
              description="Install the CLI, reuse local app credentials, and persist a working session."
            />
            <SurfaceCard
              icon={<BookOpen className="size-4" />}
              title="Read chats"
              description="Start with cache-backed reads, then switch to LOCO when you need the live path."
            />
            <SurfaceCard
              icon={<BellRing className="size-4" />}
              title="Watch events"
              description="Move from polling to reconnect-aware event monitoring with local hooks."
            />
            <SurfaceCard
              icon={<Webhook className="size-4" />}
              title="Automate carefully"
              description="Keep side effects narrow, explicit, and visible to the operator."
            />
          </div>

          <div className="mt-5 rounded-xl border border-white/10 bg-white/5 p-4">
            <div className="mb-2 text-xs font-medium uppercase tracking-wide text-white/50">Terminal</div>
            <pre className="overflow-x-auto font-mono text-sm text-white/80">
              <code>{`brew install openkakao-cli
openkakao-cli ax-read "Me" --json
openkakao-cli local-send "Me" "hi"`}</code>
            </pre>
          </div>
        </div>

        <div className="border-l border-white/10 px-4 py-5 text-sm text-white/65">
          <div className="mb-4 flex items-center gap-2 text-white/80">
            <BookOpen className="size-4" />
            On this page
          </div>
          <TocItem label="Introduction" active />
          <TocItem label="Install and allowlist" />
          <TocItem label="Read chats" />
          <TocItem label="Watch events" />
          <TocItem label="Automation" />
          <TocItem label="Next paths" />
        </div>
      </div>
    </div>
  );
}

function SidebarGroup({
  title,
  items,
  active,
}: {
  title: string;
  items: string[];
  active?: string;
}) {
  return (
    <div className="mb-5">
      <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-white/35">{title}</div>
      <div className="space-y-1.5">
        {items.map((item) => (
          <div
            key={item}
            className={cn(
              'rounded-md px-3 py-2 text-[13px] leading-5 text-white/70',
              active === item && 'bg-white/8 text-white',
            )}
          >
            {item}
          </div>
        ))}
      </div>
    </div>
  );
}

function SurfaceCard({
  icon,
  title,
  description,
}: {
  icon: ReactNode;
  title: string;
  description: string;
}) {
  return (
    <div className="rounded-xl border border-white/10 bg-white/4 p-4">
      <div className="mb-3 flex size-8 items-center justify-center rounded-lg border border-white/10 bg-white/5 text-white/75">
        {icon}
      </div>
      <div className="mb-2 text-[15px] leading-5 font-medium text-white">{title}</div>
      <p className="text-[13px] leading-6 text-white/60">{description}</p>
    </div>
  );
}

function TocItem({ label, active = false }: { label: string; active?: boolean }) {
  return (
    <div
      className={cn(
        'border-l px-3 py-1.5 text-[12px] leading-5',
        active ? 'border-[#F58B54] text-[#F58B54]' : 'border-white/10',
      )}
    >
      {label}
    </div>
  );
}

function DocsSurface() {
  return (
    <div className="grid h-[398px] grid-cols-[210px_minmax(0,1fr)] overflow-hidden rounded-xl border bg-[#181818] text-white">
      <div className="border-r border-white/10 bg-black/10 px-4 py-4">
        <SidebarGroup title="Overview" items={['Why OpenKakao', 'Limitations']} active="Why OpenKakao" />
        <SidebarGroup title="Getting Started" items={['Getting Started', 'Configuration', 'Troubleshooting']} />
        <SidebarGroup title="Security" items={['Trust Model', 'Data & Credentials']} />
      </div>
      <div className="overflow-hidden px-6 py-5">
        <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-[#F58B54]">Overview</div>
        <h3 className="mb-3 text-[1.9rem] leading-tight font-semibold">Why OpenKakao</h3>
        <p className="mb-5 max-w-2xl text-[14px] leading-7 text-white/60">
          KakaoTalk already holds requests, status updates, and coordination. OpenKakao gives technical users
          a local CLI surface around that context.
        </p>
        <div className="grid gap-4 md:grid-cols-2">
          <SurfaceCard
            icon={<Search className="size-4" />}
            title="Read and inspect"
            description="Turn opaque chat history into searchable local data."
          />
          <SurfaceCard
            icon={<Database className="size-4" />}
            title="Export and persist"
            description="Move message slices into JSON, SQLite, and your own tools."
          />
          <SurfaceCard
            icon={<BellRing className="size-4" />}
            title="Watch events"
            description="Respond to new messages without polling loops."
          />
          <SurfaceCard
            icon={<Shield className="size-4" />}
            title="Keep the boundary explicit"
            description="Know what stays local and when trust expands."
          />
        </div>
      </div>
    </div>
  );
}

function HistorySurface() {
  return (
    <div className="grid h-[398px] grid-cols-[210px_minmax(0,1fr)] overflow-hidden rounded-xl border bg-[#181818] text-white">
      <div className="border-r border-white/10 bg-black/10 px-4 py-4">
        <SidebarGroup title="History" items={['Recent reads', 'JSON export']} active="Recent reads" />
        <SidebarGroup title="Storage" items={['SQLite', 'Search indexes', 'Local files']} />
        <SidebarGroup title="Boundary" items={['Stay local first', 'Review before send']} />
      </div>
      <div className="overflow-hidden px-6 py-5">
        <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-[#F58B54]">History</div>
        <h3 className="mb-3 text-[1.9rem] leading-tight font-semibold">Turn chats into local data.</h3>
        <p className="mb-5 max-w-2xl text-[14px] leading-7 text-white/60">
          Pull message slices into JSON and persist them in the tools you already control.
        </p>
        <div className="grid gap-4 md:grid-cols-[1.15fr_0.85fr]">
          <div className="rounded-xl border border-white/10 bg-black/10 p-4">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-medium text-white/80">
              <Database className="size-4" />
              Local message history
            </div>
            <div className="grid gap-2 font-mono text-xs text-white/70">
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">09:13  team-room   "deploy is green"</div>
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">09:17  alerts      "[Photo] screenshot.png"</div>
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">09:24  ops         "urgent: check webhook receiver"</div>
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">09:41  support     "export last 50 messages"</div>
            </div>
          </div>
          <div className="rounded-xl border border-white/10 bg-black/10 p-4">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-medium text-white/80">
              <Terminal className="size-4" />
              Query surface
            </div>
            <pre className="rounded-lg border border-white/8 bg-white/4 p-3 font-mono text-xs text-white/75">{`openkakao-cli export \\
  --chat-id 900000000000001 \\
  --format json \\
  --limit 50 > messages.json

jq '.[] | {author, message}' messages.json`}</pre>
          </div>
        </div>
      </div>
    </div>
  );
}

function AutomationSurface() {
  return (
    <div className="grid h-[398px] grid-cols-[210px_minmax(0,1fr)] overflow-hidden rounded-xl border bg-[#181818] text-white">
      <div className="border-r border-white/10 bg-black/10 px-4 py-4">
        <SidebarGroup title="Automation" items={['Workflow ladder', 'Hooks', 'Webhooks']} active="Workflow ladder" />
        <SidebarGroup title="Patterns" items={['Review queues', 'Signed delivery']} />
        <SidebarGroup title="Boundary" items={['Operator approval', 'Narrow side effects']} />
      </div>
      <div className="overflow-hidden px-6 py-5">
        <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-[#F58B54]">Automation</div>
        <h3 className="mb-3 text-[1.9rem] leading-tight font-semibold">Automate with a clear boundary.</h3>
        <p className="mb-5 max-w-2xl text-[14px] leading-7 text-white/60">
          Use watch, hooks, and signed webhooks when polling is no longer enough.
        </p>
        <div className="grid gap-4 md:grid-cols-[0.8fr_1.2fr]">
          <div className="rounded-xl border border-white/10 bg-black/10 p-4">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-medium text-white/80">
              <Webhook className="size-4" />
              Workflow ladder
            </div>
            <div className="space-y-2 text-[13px] leading-5 text-white/70">
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">1. read recent context</div>
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">2. watch for a narrow event</div>
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">3. classify or summarize locally</div>
              <div className="rounded-lg border border-white/8 bg-white/4 px-3 py-2">4. review before outbound send</div>
            </div>
          </div>
          <div className="rounded-xl border border-white/10 bg-black/10 p-4">
            <div className="mb-3 flex items-center gap-2 text-[13px] font-medium text-white/80">
              <Terminal className="size-4" />
              Hook example
            </div>
            <pre className="rounded-lg border border-white/8 bg-white/4 p-3 font-mono text-xs text-white/75">{`openkakao-cli --unattended \\
  --allow-watch-side-effects \\
  watch \\
  --hook-chat-id 900000000000001 \\
  --hook-keyword urgent \\
  --hook-cmd './handle-event.sh'`}</pre>
            <div className="mt-4 grid gap-3 md:grid-cols-3">
              <SurfaceCard icon={<BellRing className="size-4" />} title="Watch" description="Reconnect-aware event stream." />
              <SurfaceCard icon={<Webhook className="size-4" />} title="Hook" description="Local command or signed webhook." />
              <SurfaceCard icon={<Shield className="size-4" />} title="Review" description="Explicit operator boundary." />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

const writingTabs = [
  { name: 'Operator', value: 'operator' },
  { name: 'Developer', value: 'developer' },
  { name: 'Automation', value: 'automation' },
] as const;

export function Writing({
  tabs,
}: {
  tabs: Record<(typeof writingTabs)[number]['value'], ReactNode>;
}) {
  const [tab, setTab] = useState<(typeof writingTabs)[number]['value']>('operator');

  return (
    <div className="col-span-full my-20">
      <h2 className="mb-8 text-center text-4xl font-medium tracking-tight text-brand">
        One surface, many workflows.
      </h2>
      <p className="mx-auto mb-8 w-full max-w-[800px] text-center">
        OpenKakao is most useful when you use it as a narrow local bridge: inspect first, structure next,
        and automate only where the boundary is explicit.
      </p>
      <div className="mb-6 flex items-center justify-center gap-4 text-fd-muted-foreground">
        {writingTabs.map((item) => (
          <Fragment key={item.value}>
            <ArrowRight className="size-4 first:hidden" />
            <button
              className={cn('text-lg font-medium transition-colors', item.value === tab && 'text-brand')}
              onClick={() => setTab(item.value)}
            >
              {item.name}
            </button>
          </Fragment>
        ))}
      </div>
      {Object.entries(tabs).map(([key, value]) => (
        <div key={key} aria-hidden={key !== tab} className={cn('animate-fd-fade-in', key !== tab && 'hidden')}>
          {value}
        </div>
      ))}
    </div>
  );
}

export function AgnosticBackground() {
  return (
    <div className="absolute inset-0 -z-1 overflow-hidden rounded-2xl">
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_20%_20%,rgba(255,168,97,0.2),transparent_28%),radial-gradient(circle_at_80%_80%,rgba(198,187,88,0.16),transparent_32%)] dark:bg-[radial-gradient(circle_at_20%_20%,rgba(255,243,131,0.12),transparent_28%),radial-gradient(circle_at_80%_80%,rgba(252,119,68,0.14),transparent_32%)]" />
      <div className="absolute inset-0 bg-[linear-gradient(to_right,transparent_0%,rgba(255,255,255,0.08)_50%,transparent_100%)] opacity-40 dark:opacity-20" />
    </div>
  );
}
