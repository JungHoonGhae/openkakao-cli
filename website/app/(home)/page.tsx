import Link from 'next/link';
import { cva } from 'class-variance-authority';
import {
  BatteryChargingIcon,
  BellRing,
  BookIcon,
  FileIcon,
  SearchIcon,
  ShieldCheck,
  TimerIcon,
  Webhook,
} from 'lucide-react';
import { cn } from '@/lib/cn';
import { Marquee } from '@/app/(home)/marquee';
import { CodeBlock } from '@/components/code-block';
import {
  AgnosticBackground,
  Hero,
  PreviewImages,
  Writing,
} from '@/app/(home)/page.client';

const headingVariants = cva('font-medium tracking-tight', {
  variants: {
    variant: {
      h2: 'text-3xl lg:text-4xl',
      h3: 'text-xl lg:text-2xl',
    },
  },
});

const buttonVariants = cva(
  'inline-flex justify-center rounded-full px-5 py-3 font-medium tracking-tight transition-colors',
  {
    variants: {
      variant: {
        primary: 'bg-brand text-brand-foreground hover:bg-brand-200',
        secondary: 'border bg-fd-secondary text-fd-secondary-foreground hover:bg-fd-accent',
      },
    },
    defaultVariants: { variant: 'primary' },
  },
);

const cardVariants = cva('rounded-2xl bg-origin-border p-6 text-sm shadow-lg', {
  variants: {
    variant: {
      secondary: 'bg-brand-secondary text-brand-secondary-foreground',
      default: 'border bg-fd-card',
    },
  },
  defaultVariants: { variant: 'default' },
});

const feedback = [
  {
    title: 'Read',
    message: 'Read recent cache-backed messages fast, or switch to LOCO when the workflow needs the real history path.',
  },
  {
    title: 'Watch',
    message: 'Move from polling to reconnect-aware real-time monitoring when latency starts to matter.',
  },
  {
    title: 'Export',
    message: 'Export selected message slices into JSON, SQLite, search indexes, and local tools you already control.',
  },
  {
    title: 'Send carefully',
    message: 'Keep outbound actions narrow, explicit, and close to the operator instead of hiding them behind a relay.',
  },
];

export default function Page() {
  return (
    <main className="pb-6 pt-4 text-landing-foreground dark:text-landing-foreground-dark md:pb-12">
      <div className="relative mx-auto flex h-[70vh] max-h-[900px] min-h-[600px] w-full max-w-[1400px] overflow-hidden rounded-2xl border bg-origin-border">
        <Hero />
        <div className="z-2 flex size-full flex-col px-4 md:p-12 max-md:items-center max-md:text-center">
          <p className="mt-12 w-fit rounded-full border border-brand/50 p-2 text-xs font-medium text-brand">
            beta · unofficial KakaoTalk CLI for macOS
          </p>
          <h1 className="my-8 text-4xl leading-tighter font-medium xl:mb-12 xl:text-5xl">
            Turn KakaoTalk into a
            <br />
            practical <span className="text-brand">local workflow</span>.
          </h1>
          <div className="flex w-fit flex-row flex-wrap items-center justify-center gap-4">
            <Link
              href="/docs/getting-started/quickstart"
              className={cn(buttonVariants(), 'max-sm:text-sm')}
            >
              Getting Started
            </Link>
            <Link
              href="/docs/security/trust-model"
              className={cn(buttonVariants({ variant: 'secondary' }), 'max-sm:text-sm')}
            >
              Security
            </Link>
          </div>
        </div>
      </div>

      <div className="mx-auto mt-12 grid w-full max-w-[1400px] grid-cols-1 gap-10 px-6 md:px-12 lg:mt-20 lg:grid-cols-2">
        <div className="col-span-full">
          <p className="text-2xl font-light leading-snug tracking-tight md:text-3xl xl:text-4xl">
            Turn KakaoTalk into a <span className="font-medium text-brand">local workflow surface</span>.
          </p>
          <p className="mt-3 max-w-3xl text-base leading-7 text-fd-muted-foreground md:text-lg">
            No login, 0.4 s per verified send, just the official app you are already signed in to.
          </p>
        </div>

        <InstallCta />

        <Feedback />
        <Aesthetics />
        <AnybodyCanWrite />
        <ForEngineers />
        <OpenSource />
      </div>
    </main>
  );
}

function InstallCta() {
  return (
    <div className="relative col-span-full overflow-hidden rounded-2xl border bg-[#111111] px-6 py-8 text-white shadow-lg md:px-8 md:py-10">
      <div className="pointer-events-none absolute inset-0">
        <div className="absolute left-[12%] top-[42%] h-[420px] w-[520px] rounded-full bg-[#F1E57A]/22 blur-[120px]" />
        <div className="absolute right-[8%] bottom-[-8%] h-[360px] w-[460px] rounded-full bg-[#F58B54]/22 blur-[120px]" />
        <div className="absolute inset-x-0 bottom-0 h-[42%] bg-gradient-to-b from-transparent via-[#111111]/30 to-[#111111]" />
      </div>
      <div className="relative z-1 grid gap-10 lg:grid-cols-[0.95fr_1.05fr] lg:items-start">
        <div className="max-w-xl">
          <h2 className={cn(headingVariants({ variant: 'h2', className: 'mb-5 text-balance' }))}>
            Install in minutes.
          </h2>
          <p className="mb-8 text-lg leading-8 text-white/70">
            Install the CLI, grant Accessibility, and send your first verified message. No server login.
          </p>
          <div className="flex flex-wrap gap-3">
            <Link href="/docs/getting-started/quickstart" className={cn(buttonVariants(), 'bg-[#F1E57A] text-neutral-950 hover:bg-[#f5e98f]')}>
              Getting Started
            </Link>
            <Link href="/docs/getting-started/installation" className={cn(buttonVariants({ variant: 'secondary' }), 'border-white/10 bg-white/5 text-white hover:bg-white/10')}>
              Installation
            </Link>
          </div>
        </div>

        <div className="relative flex min-h-[360px] items-end justify-center">
          <div className="absolute inset-x-[12%] top-[8%] h-[78%] rounded-[36px] bg-black/45 blur-3xl" />
          <div className="relative w-full max-w-[620px] rounded-[34px] border border-white/10 bg-[#0F1014] px-8 py-7 shadow-2xl shadow-black/40 [transform:perspective(1400px)_rotateX(16deg)_rotateZ(-7deg)]">
            <div className="mb-6 flex items-center gap-3 text-sm text-white/55">
              <div className="size-3 rounded-full bg-[#c6b8ff]" />
              terminal
            </div>
            <div className="font-mono text-lg text-white/90 md:text-[28px] md:leading-[1.4]">
              <span className="text-white/50">brew install </span>
              <span>openkakao-cli</span>
            </div>
            <div className="mt-3 font-mono text-sm text-white/55 md:text-base">openkakao-cli local-send "Me" "hi"</div>
            <div className="mt-10 grid gap-2 text-sm text-white/55">
              <div>Finding the chat through Accessibility...</div>
              <div>Verifying the new bubble...</div>
              <div className="text-[#F1E57A]">Delivered and verified in 0.4 s.</div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function Feedback() {
  return (
    <>
      <div className={cn(cardVariants())}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          A practical CLI surface.
        </h3>
        <p className="mb-6">
          Use OpenKakao when KakaoTalk already holds the context, but the workflow surface around it is still too limited.
        </p>
        <Link href="/docs/overview/why-openkakao" className={cn(buttonVariants())}>
          Why it exists
        </Link>
      </div>
      <div className={cn(cardVariants({ variant: 'secondary', className: 'relative p-0' }))}>
        <div className="absolute inset-0 z-2 rounded-2xl inset-shadow-[0_10px_60px] inset-shadow-brand-secondary" />
        <Marquee className="p-8">
          {feedback.map((item) => (
            <div
              key={item.title}
              className="flex w-[320px] flex-col rounded-xl border bg-fd-card p-4 text-landing-foreground shadow-lg"
            >
              <p className="text-sm font-medium">{item.title}</p>
              <p className="mt-3 text-sm whitespace-pre-wrap">{item.message}</p>
            </div>
          ))}
        </Marquee>
      </div>
    </>
  );
}

function Aesthetics() {
  return (
    <>
      <div className={cn(cardVariants({ variant: 'secondary', className: 'flex items-center justify-center p-0' }))}>
        <PreviewImages />
      </div>
      <div className={cn(cardVariants(), 'flex flex-col')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          One docs shell, several workflow surfaces.
        </h3>
        <p className="mb-4">Move from guides to command reference to trust boundary without leaving the same shell.</p>
        <p className="mb-4">The structure follows official Fumadocs patterns, but the workflow model is now fully OpenKakao.</p>
        <CodeBlock
          code={'openkakao-cli loco-chats\nopenkakao-cli loco-read <chat_id> -n 20 --json\nopenkakao-cli watch --chat-id <chat_id>'}
          lang="bash"
          className="my-0"
        />
      </div>
    </>
  );
}

function AnybodyCanWrite() {
  return (
    <Writing
      tabs={{
        operator: (
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
            <CodeBlock
              code={'openkakao-cli unread --json\nopenkakao-cli loco-read <chat_id> -n 50 --json | jq .'}
              lang="bash"
            />
            <div className="max-lg:row-start-1">
              <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>
                Inspect first.
              </h3>
              <p>Turn message streams into readable, reviewable input before you automate side effects.</p>
              <ul className="mt-8 list-inside list-disc text-xs">
                <li>Unread triage</li>
                <li>History export</li>
                <li>Review queues</li>
                <li>Operator summaries</li>
              </ul>
            </div>
          </div>
        ),
        developer: (
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
            <CodeBlock
              code={'openkakao-cli watch --hook-cmd ./handle-event.sh\nopenkakao-cli watch --webhook-url https://hooks.example.com/openkakao'}
              lang="bash"
            />
            <div className="max-lg:row-start-1">
              <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>
                Compose your own stack.
              </h3>
              <p>Use hooks, webhooks, JSON output, and local persistence to bridge KakaoTalk into the rest of your tooling.</p>
              <ul className="mt-8 list-inside list-disc text-xs">
                <li>Command hooks</li>
                <li>Webhook delivery</li>
                <li>SQLite and search indexes</li>
                <li>CLI-first automation</li>
              </ul>
            </div>
          </div>
        ),
        automation: (
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
            <CodeBlock
              code={'1. read\n2. summarize\n3. classify\n4. review\n5. send only if needed'}
              lang="txt"
            />
            <div className="max-lg:row-start-1">
              <h3 className={cn(headingVariants({ variant: 'h3', className: 'my-4' }))}>
                Keep the boundary explicit.
              </h3>
              <p>The project is useful because it stays close to the real app. That is also why the trust boundary has to stay explicit.</p>
              <ul className="mt-8 list-inside list-disc text-xs">
                <li>Local-first by default</li>
                <li>REST and LOCO documented separately</li>
                <li>Unattended mode is explicit</li>
                <li>Outbound automation stays narrow</li>
              </ul>
            </div>
          </div>
        ),
      }}
    />
  );
}

function StoryCard() {
  return (
    <div className="relative col-span-full min-h-[570px] rounded-2xl border px-2 py-6 shadow-md">
      <div className="absolute inset-0 -z-1 rounded-2xl bg-[radial-gradient(circle_at_top_left,rgba(255,168,97,0.22),transparent_30%),radial-gradient(circle_at_80%_80%,rgba(198,187,88,0.18),transparent_28%),linear-gradient(180deg,rgba(18,18,18,0.9),rgba(10,10,10,1))]" />
      <div className="mx-auto grid w-full max-w-[900px] gap-5 px-4 py-4 lg:grid-cols-[248px_minmax(0,1fr)] lg:items-start">
        <div className="rounded-xl border bg-black/20 p-5 text-start shadow-xl shadow-black/15 backdrop-blur-sm">
          <div className="mb-4 text-xs font-medium uppercase tracking-[0.2em] text-white/40">
            Operator Notes
          </div>
          <h3 className="mb-3 text-lg font-semibold tracking-tight text-white">
            Keep the workflow narrow.
          </h3>
          <p className="mb-4 text-sm leading-6 text-white/65">
            Treat OpenKakao as a local operator tool. Read first, review next, and expand automation only when the boundary is clear.
          </p>
          <div className="space-y-2.5">
            {[
              ['Read locally', 'Start with message history and account state before any side effects.'],
              ['Escalate carefully', 'Use webhooks only when the workflow truly needs to leave the machine.'],
            ].map(([title, copy]) => (
              <div key={title} className="rounded-lg border border-white/10 bg-white/4 p-3">
                <p className="text-sm font-medium text-white">{title}</p>
                <p className="mt-1.5 text-xs leading-5 text-white/55">{copy}</p>
              </div>
            ))}
          </div>
          <div className="mt-4 rounded-lg border border-white/10 bg-black/30 px-3 py-2 font-mono text-xs text-white/65">
            openkakao-cli auth-status --json
          </div>
        </div>

        <div className="rounded-xl border bg-fd-card/80 p-2 text-start shadow-xl shadow-black/20 backdrop-blur-md dark:bg-fd-card/50">
          <div className="px-3 pt-3">
            <h2 className={cn(headingVariants({ className: 'mb-4', variant: 'h3' }))}>Why this exists</h2>
            <p className="mb-4 text-sm">
              KakaoTalk already holds requests, updates, and coordination. OpenKakao exists because the workflow surface around that context is still structurally limited.
            </p>
            <Link href="/docs/overview/why-openkakao" className={cn(buttonVariants({ className: 'mb-4 py-2 text-sm' }))}>
              Explore
            </Link>
          </div>
          <div className="rounded-xl border bg-fd-secondary p-4">
            <p className="text-sm font-medium">The value is composition, not one command.</p>
            <p className="mt-3 text-sm text-fd-muted-foreground">
              Read, export, watch, classify, review, and send only if the workflow still needs it.
            </p>
          </div>
          <div className="mt-3 grid gap-3 rounded-xl border bg-black/70 p-4 text-white md:grid-cols-3">
            {[
              ['Read', 'Pull real chat context into a scriptable local surface.'],
              ['Watch', 'React to events when latency matters.'],
              ['Review', 'Keep outbound action narrow and explicit.'],
            ].map(([title, copy]) => (
              <div key={title} className="rounded-lg border border-white/10 bg-white/4 p-3">
                <p className="text-sm font-medium">{title}</p>
                <p className="mt-2 text-xs leading-5 text-white/60">{copy}</p>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function ForEngineers() {
  return (
    <>
      <h2 className={cn(headingVariants({ variant: 'h2', className: 'col-span-full mb-4 text-center text-brand' }))}>
        Docs For Operators.
      </h2>
      <StoryCard />

      <div className={cn(cardVariants(), 'relative z-2 flex flex-col overflow-hidden')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Two transport surfaces, one CLI.
        </h3>
        <p className="mb-20">
          REST stays cheap and cache-backed. LOCO handles real chat workflows, watch mode, and sending.
        </p>
        <div className="mt-auto flex w-fit flex-row gap-2 rounded-xl bg-brand p-2 text-brand-foreground">
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">REST</div>
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">LOCO</div>
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">Hooks</div>
          <div className="rounded-lg bg-black/10 px-3 py-2 text-sm font-medium">Webhooks</div>
        </div>
        <AgnosticBackground />
      </div>

      <div className={cn(cardVariants(), 'flex flex-col')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Composable primitives.
        </h3>
        <p className="mb-8">
          The CLI stays intentionally small: read, watch, search, export, send, and auth recovery.
        </p>
        <div className="mt-auto flex flex-col gap-2 @container mask-[linear-gradient(to_bottom,white,transparent)]">
          {[
            { name: 'ax-read', description: 'Read an open chat in 0.2 s through Accessibility. No login.' },
            { name: 'loco-read', description: 'Reliable history fetches for exports and automation.' },
            { name: 'watch', description: 'Real-time monitoring with reconnect handling and narrow side effects.' },
            { name: 'local-send', description: 'Send to an allowlisted chat in 0.4 s, verified by its new bubble.' },
            { name: 'ax-login', description: 'Sign a signed-out KakaoTalk back in from the keychain.' },
          ].map((item) => (
            <div
              key={item.name}
              className="flex flex-col gap-2 border border-dashed border-brand-secondary p-2 text-sm @lg:flex-row @lg:items-center"
            >
              <p className="font-medium text-nowrap">{item.name}</p>
              <p className="text-xs @lg:flex-1 @lg:text-end">{item.description}</p>
            </div>
          ))}
        </div>
      </div>

      <div className={cn(cardVariants(), 'flex flex-col')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Adopts your local stack.
        </h3>
        <p className="mb-4">
          OpenKakao is strongest when it feeds tools you already trust: `jq`, `sqlite`, launchd, local agents, search indexes, and narrow webhook receivers.
        </p>
        <div className="mb-6 flex w-fit flex-row items-center gap-4">
          {['jq', 'sqlite', 'launchd', 'webhooks'].map((item) => (
            <span key={item} className="text-sm text-brand">
              {item}
            </span>
          ))}
        </div>
        <CodeBlock
          code={`openkakao-cli loco-read <chat_id> --all --json > history.json
jq '.[] | {author, message}' history.json
sqlite3 chat.db '.import history.json messages'`}
          lang="bash"
          className="my-0"
        />
      </div>

      <div className={cn(cardVariants({ className: 'relative min-h-[400px] overflow-hidden z-2' }))}>
        <div className="absolute inset-0 -z-1 bg-[radial-gradient(circle_at_15%_20%,rgba(254,229,0,0.16),transparent_18%),radial-gradient(circle_at_85%_80%,rgba(245,139,84,0.22),transparent_25%),linear-gradient(180deg,rgba(17,17,17,0.9),rgba(6,6,6,1))]" />
        <div className="absolute left-4 top-8 flex w-[70%] flex-col rounded-xl border bg-neutral-50/80 p-2 text-neutral-800 shadow-lg shadow-black backdrop-blur-lg dark:bg-neutral-900/80 dark:text-neutral-200">
          <p className="mb-2 border-b px-2 pb-2 font-medium text-neutral-500 dark:text-neutral-400">
            Local workflow
          </p>
          {['Unread review', 'JSON export', 'Webhook receiver', 'Operator summary'].map((page) => (
            <div key={page} className="flex items-center gap-2 rounded-lg p-2 hover:bg-neutral-400/20">
              <FileIcon className="size-4 stroke-neutral-500 dark:stroke-neutral-400" />
              <span className="text-sm">{page}</span>
              <div className="ms-auto rounded-full bg-brand px-3 py-1 font-mono text-xs text-brand-foreground">
                Step
              </div>
            </div>
          ))}
        </div>
        <div className="absolute bottom-8 right-4 flex w-[70%] flex-col rounded-xl border bg-neutral-100 text-neutral-800 shadow-lg shadow-black dark:bg-neutral-900 dark:text-neutral-200">
          <div className="border-b px-4 py-2 font-medium text-neutral-500 dark:text-neutral-400">
            CLI
          </div>
          <pre className="overflow-auto p-4 text-base text-neutral-800 dark:text-neutral-400">{`openkakao-cli unread --json
openkakao-cli watch --hook-cmd ./handle-event.sh
openkakao-cli send <chat_id> "done"`}</pre>
        </div>
      </div>

      <div className={cn(cardVariants(), 'flex flex-col max-md:pb-0')}>
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
          Search the exact command surface.
        </h3>
        <p className="mb-6">
          The landing explains where the CLI helps. Search takes you to the exact command, flag, and
          risk boundary when you need to implement.
        </p>
        <Link href="/docs" className={cn(buttonVariants({ className: 'mb-8 w-fit' }))}>
          Open docs
        </Link>
        <SearchPanel />
      </div>

      <div className={cn(cardVariants(), 'flex flex-col overflow-hidden p-0')}>
        <div className="mb-2 p-6">
          <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>
            The workflow docs for OpenKakao
          </h3>
          <p className="mb-6">
            From quickstart to trust model to command detail, the site keeps the official Fumadocs rhythm while replacing the example surface with OpenKakao's actual workflow model.
          </p>
          <Link href="/docs/cli/overview" className={cn(buttonVariants({ className: 'w-fit' }))}>
            Command reference
          </Link>
        </div>
        <div className="mt-auto flex flex-1 items-stretch bg-[#161616] p-4 text-white">
          <div className="grid w-full grid-cols-[180px_minmax(0,1fr)] overflow-hidden rounded-xl border border-white/10">
            <div className="border-r border-white/10 bg-black/15 px-3 py-4">
              <div className="mb-3 rounded-lg border border-[#FEE500]/20 bg-[#FEE500]/10 px-3 py-2 text-sm font-medium text-[#FEE500]">
                Getting Started
              </div>
              {['Getting Started', 'Installation', 'Authentication', 'Configuration'].map((item, i) => (
                <div
                  key={item}
                  className={cn(
                    'mb-1 rounded-md px-3 py-2 text-sm text-white/65',
                    i === 0 && 'bg-white/8 text-white',
                  )}
                >
                  {item}
                </div>
              ))}
            </div>
            <div className="px-5 py-4">
              <div className="mb-3 text-sm text-[#F58B54]">Getting Started</div>
              <h4 className="mb-2 text-2xl font-semibold">Install, authenticate, and read.</h4>
              <p className="mb-5 max-w-xl text-sm text-white/60">
                Start with local app state, confirm the session, then use `read` or `loco-read` depending on the workflow boundary.
              </p>
              <div className="grid gap-3 md:grid-cols-2">
                {[
                  ['Install', 'Set up the CLI and keep the environment local.'],
                  ['Authenticate', 'Reuse credentials from the real app and persist them carefully.'],
                  ['Read', 'Inspect cache-backed or LOCO-backed message paths.'],
                  ['Automate', 'Move to watch, hooks, and webhooks only when needed.'],
                ].map(([title, copy]) => (
                  <div key={title} className="rounded-lg border border-white/10 bg-white/4 p-3">
                    <p className="text-sm font-medium">{title}</p>
                    <p className="mt-2 text-xs leading-5 text-white/60">{copy}</p>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

function SearchPanel() {
  const items = [
    ['Getting Started', 'Install, authenticate, and read your first chat.'],
    ['read / loco-read', 'Choose between cache-backed reads and full history fetches.'],
    ['watch', 'Real-time monitoring, hooks, webhooks, and reconnect boundaries.'],
    ['Security', 'Trust model, credentials, and safe usage boundaries.'],
  ];

  return (
    <div className="mt-auto flex select-none flex-col rounded-xl border bg-fd-popover mask-[linear-gradient(to_bottom,white_40%,transparent_90%)] max-md:-mx-4">
      <div className="inline-flex items-center gap-2 px-4 py-3 text-sm text-fd-muted-foreground">
        <SearchIcon className="size-4" />
        Search...
      </div>
      <div className="border-t p-2">
        {items.map(([title, description], i) => (
          <div key={title} className={cn('rounded-md p-2 text-sm text-fd-popover-foreground', i === 0 && 'bg-fd-accent')}>
            <div className="flex flex-row items-center gap-2">
              <BookIcon className="size-4 text-fd-muted-foreground" />
              <p>{title}</p>
            </div>
            <p className="mt-2 ps-6 text-xs text-fd-muted-foreground">{description}</p>
          </div>
        ))}
      </div>
    </div>
  );
}

function OpenSource() {
  return (
    <>
      <h2 className={cn(headingVariants({ variant: 'h2', className: 'col-span-full mt-8 mb-4 text-center text-brand' }))}>
        Operate With Clear Boundaries.
      </h2>

      <div className={cn(cardVariants({ className: 'flex flex-col' }))}>
        <ShieldCheck className="mb-4 text-brand" />
        <h3 className={cn(headingVariants({ variant: 'h3', className: 'mb-6' }))}>Security is part of the product.</h3>
        <p className="mb-8">
          OpenKakao is useful because it stays close to the real app. The docs treat security, limitations, and unattended policy as first-class topics for the same reason.
        </p>
        <div className="mb-8 flex flex-row items-center gap-2">
          <Link href="/docs/security/trust-model" className={cn(buttonVariants({ variant: 'primary' }))}>
            Security
          </Link>
          <Link href="/docs/security/safe-usage" className={cn(buttonVariants({ variant: 'secondary' }))}>
            Safe usage
          </Link>
        </div>
      </div>

      <div className={cn(cardVariants({ className: 'flex flex-col p-0 pt-8' }))}>
        <h2 className="mb-4 text-center font-mono text-3xl font-extrabold uppercase lg:text-4xl">
          Build Your Workflow
        </h2>
        <p className="mb-8 text-center font-mono text-xs opacity-50">
          local, scriptable, and explicit about its boundaries.
        </p>
        <div className="mt-auto h-[200px] overflow-hidden bg-gradient-to-b from-brand-secondary/10 p-8">
          <div className="mx-auto size-[500px] rounded-full bg-radial-[circle_at_0%_100%] from-brand-secondary to-transparent from-60%" />
        </div>
      </div>

      <ul className={cn(cardVariants({ className: 'col-span-full flex flex-col gap-6' }))}>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <BatteryChargingIcon className="size-5" />
            Local-first by default.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            The intended trust boundary is your machine to Kakao, not your machine to an OpenKakao backend.
          </span>
        </li>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <BellRing className="size-5" />
            Event-driven when needed.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            Use watch mode when polling is no longer enough, but keep retries and delivery guarantees in your own wrapper.
          </span>
        </li>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <Webhook className="size-5" />
            Narrow side effects.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            Hooks and webhooks are explicit surfaces, not hidden background behavior.
          </span>
        </li>
        <li>
          <span className="flex flex-row items-center gap-2 font-medium">
            <TimerIcon className="size-5" />
            Fast to start.
          </span>
          <span className="mt-2 text-sm text-fd-muted-foreground">
            Install, authenticate, and read the first chat in minutes, then deepen only the workflows you actually need.
          </span>
        </li>
        <li className="mt-auto flex flex-row flex-wrap gap-2">
          <Link href="/docs" className={cn(buttonVariants())}>
            Read docs
          </Link>
          <a
            href="https://github.com/JungHoonGhae/openkakao-cli"
            rel="noreferrer noopener"
            className={cn(buttonVariants({ variant: 'secondary' }))}
          >
            Open GitHub
          </a>
        </li>
      </ul>
    </>
  );
}
