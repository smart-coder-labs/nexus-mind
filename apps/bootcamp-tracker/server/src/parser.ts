import fs from 'fs';

export interface ParsedTopic {
  number: number;
  title: string;
  icon: string;
  color: string;
  sections: ParsedSection[];
}

export interface ParsedSection {
  title: string;
  subtopics: ParsedSubtopic[];
  resources: ParsedResource[];
}

export interface ParsedSubtopic {
  label: string;
  priority: 'P0' | 'P1' | 'P2';
  estimated_time: string;
  completed: boolean;
}

export interface ParsedResource {
  type: 'paper' | 'book' | 'course' | 'repo';
  label: string;
  url: string | null;
}

export interface ParsedRoadmapDay {
  week: number;
  day: number;
  title: string;
  blocks: { time: string; activities: string[] }[];
}

export interface ParsedData {
  topics: ParsedTopic[];
  roadmapDays: ParsedRoadmapDay[];
}

const TOPIC_COLORS: Record<number, string> = {
  1: '#FF6B6B',
  2: '#4ECDC4',
  3: '#45B7D1',
  4: '#96CEB4',
  5: '#FFEAA7',
  6: '#DDA0DD',
  7: '#F0E68C',
  8: '#98D8C8',
  9: '#FFB347',
  10: '#87CEEB',
  11: '#DEB887',
  12: '#9B89B8',
};

function extractLinkLabel(text: string): { label: string; url: string | null } {
  // Match [label](url)
  const linkMatch = text.match(/\[([^\]]+)\]\(([^)]+)\)/);
  if (linkMatch) {
    return { label: linkMatch[1], url: linkMatch[2] };
  }
  // Match *label* — author — info (italics, no link)
  const italicMatch = text.match(/^\*([^*]+)\*/);
  if (italicMatch) {
    return { label: italicMatch[1], url: null };
  }
  // Plain text
  const clean = text.replace(/\*\*/g, '').replace(/\*/g, '').trim();
  return { label: clean || text.trim(), url: null };
}

function parseResourceType(header: string): 'paper' | 'book' | 'course' | 'repo' {
  const h = header.toLowerCase();
  if (h.includes('paper') || h.includes('lectura')) return 'paper';
  if (h.includes('libro')) return 'book';
  if (h.includes('curso') || h.includes('recurso')) return 'course';
  if (h.includes('repo')) return 'repo';
  return 'paper';
}

function parseSubtopicRow(line: string): ParsedSubtopic | null {
  // | label | P0/P1/P2 | Xh | [ ] | or [x]
  const cells = line.split('|').map(c => c.trim()).filter(c => c !== '');
  if (cells.length < 4) return null;

  const label = cells[0];
  const priorityRaw = cells[1];
  const time = cells[2];
  const checkRaw = cells[3];

  if (!label || !['P0', 'P1', 'P2'].includes(priorityRaw)) return null;
  // Skip separator rows
  if (label.includes('---') || label === 'Subtema') return null;

  const completed = checkRaw.includes('[x]') || checkRaw.includes('[X]');
  return {
    label,
    priority: priorityRaw as 'P0' | 'P1' | 'P2',
    estimated_time: time,
    completed,
  };
}

export function parseMarkdown(filePath: string): ParsedData {
  const content = fs.readFileSync(filePath, 'utf-8');
  const lines = content.split('\n');

  const topics: ParsedTopic[] = [];
  const roadmapDays: ParsedRoadmapDay[] = [];

  let currentTopic: ParsedTopic | null = null;
  let currentSection: ParsedSection | null = null;
  let currentResourceType: 'paper' | 'book' | 'course' | 'repo' | null = null;
  let inTable = false;

  // Roadmap state
  let currentWeek = 0;
  let currentDay = 0;
  let currentDayTitle = '';
  let currentDayBlocks: { time: string; activities: string[] }[] = [];
  let currentBlock: { time: string; activities: string[] } | null = null;
  let inCodeBlock = false;
  let inRoadmap = false;

  function flushRoadmapDay() {
    if (currentDay > 0 && currentDayTitle) {
      if (currentBlock && currentBlock.activities.length > 0) {
        currentDayBlocks.push(currentBlock);
        currentBlock = null;
      }
      if (currentDayBlocks.length > 0) {
        roadmapDays.push({
          week: currentWeek,
          day: currentDay,
          title: currentDayTitle,
          blocks: currentDayBlocks,
        });
      }
      currentDayBlocks = [];
      currentDayTitle = '';
      currentDay = 0;
    }
  }

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Detect roadmap section
    if (trimmed.match(/^#\s+.*Roadmap/i) || trimmed.match(/^#\s+🗺️/)) {
      inRoadmap = true;
    }

    // Code block toggle
    if (trimmed.startsWith('```')) {
      inCodeBlock = !inCodeBlock;
      if (!inCodeBlock && inRoadmap) {
        // Exiting a code block in roadmap — flush current block
        if (currentBlock && currentBlock.activities.length > 0) {
          currentDayBlocks.push(currentBlock);
          currentBlock = null;
        }
      }
      continue;
    }

    // Inside roadmap code blocks — parse activity lines
    if (inCodeBlock && inRoadmap) {
      // Mañana (Xh): or Tarde (Xh):
      const blockMatch = trimmed.match(/^(Mañana|Tarde)\s*\([^)]+\)\s*:/);
      if (blockMatch) {
        if (currentBlock && currentBlock.activities.length > 0) {
          currentDayBlocks.push(currentBlock);
        }
        currentBlock = { time: trimmed.replace(/:$/, ''), activities: [] };
        continue;
      }
      // Activity bullet
      if (trimmed.startsWith('•') && currentBlock) {
        const activity = trimmed.replace(/^•\s*/, '').trim();
        if (activity) currentBlock.activities.push(activity);
        continue;
      }
      continue;
    }

    if (inRoadmap) {
      // Semana N header
      const weekMatch = trimmed.match(/^##\s+Semana\s+(\d+)/i);
      if (weekMatch) {
        flushRoadmapDay();
        currentWeek = parseInt(weekMatch[1]);
        continue;
      }

      // Día N: Title
      const dayMatch = trimmed.match(/^###\s+Día\s+(\d+)\s*:\s*(.+)/i);
      if (dayMatch) {
        flushRoadmapDay();
        currentDay = parseInt(dayMatch[1]);
        currentDayTitle = dayMatch[2].trim();
        currentDayBlocks = [];
        currentBlock = null;
        continue;
      }

      // Topic header inside roadmap — means roadmap section ended for content but we still track week
      if (trimmed.match(/^#\s+TEMA\s+\d+/)) {
        flushRoadmapDay();
        inRoadmap = false;
        // Fall through to topic parsing
      } else {
        continue; // Skip non-roadmap content in roadmap section
      }
    }

    // Topic header: # TEMA N: Title Icon
    const topicMatch = trimmed.match(/^#\s+TEMA\s+(\d+):\s+(.+?)\s+([\u{1F300}-\u{1FFFF}]|⚙️|🗄️|🔍|🔐|🤖|🔏|🏗️|🌐|🛡️|🚀|📢|🧠|⚡)/u);
    if (topicMatch) {
      currentSection = null;
      currentResourceType = null;
      inTable = false;

      // Duplicate detection — tema 7 appears twice in the file
      const topicNum = parseInt(topicMatch[1]);
      const existing = topics.find(t => t.number === topicNum);
      if (existing) {
        currentTopic = existing;
        continue;
      }

      const rawTitle = topicMatch[2].trim();
      const icon = topicMatch[3];

      currentTopic = {
        number: topicNum,
        title: rawTitle,
        icon,
        color: TOPIC_COLORS[topicNum] || '#888888',
        sections: [],
      };
      topics.push(currentTopic);
      continue;
    }

    // If no topic matched but line starts with # TEMA, try simpler match
    const topicSimple = trimmed.match(/^#\s+TEMA\s+(\d+):\s+(.+)/);
    if (topicSimple && !trimmed.match(/^##/)) {
      const topicNum = parseInt(topicSimple[1]);
      const existing = topics.find(t => t.number === topicNum);
      if (!existing) {
        // Extract icon from end
        const titleParts = topicSimple[2].trim();
        const iconMatch = titleParts.match(/(⚙️|🗄️|🔍|🔐|🤖|🔏|🏗️|🌐|🛡️|🚀|📢|🧠)$/u);
        const icon = iconMatch ? iconMatch[1] : '📖';
        const title = iconMatch ? titleParts.replace(icon, '').trim() : titleParts;

        currentTopic = {
          number: topicNum,
          title,
          icon,
          color: TOPIC_COLORS[topicNum] || '#888888',
          sections: [],
        };
        topics.push(currentTopic);
      } else {
        currentTopic = existing;
      }
      currentSection = null;
      currentResourceType = null;
      continue;
    }

    // Section header: ## N.M Title
    const sectionMatch = trimmed.match(/^##\s+(\d+\.\d+)\s+(.+)/);
    if (sectionMatch && currentTopic) {
      currentResourceType = null;
      inTable = false;
      currentSection = {
        title: `${sectionMatch[1]} ${sectionMatch[2]}`,
        subtopics: [],
        resources: [],
      };
      currentTopic.sections.push(currentSection);
      continue;
    }

    // Topic-level table (topics without explicit sections like 7, 8, 9, 10, 11, 12)
    if (trimmed.startsWith('|') && currentTopic && !currentSection) {
      // Create a default section for this topic
      if (currentTopic.sections.length === 0) {
        currentSection = {
          title: currentTopic.title,
          subtopics: [],
          resources: [],
        };
        currentTopic.sections.push(currentSection);
      } else {
        currentSection = currentTopic.sections[currentTopic.sections.length - 1];
      }
    }

    // Table rows
    if (trimmed.startsWith('|') && currentSection) {
      inTable = true;
      const subtopic = parseSubtopicRow(trimmed);
      if (subtopic) {
        currentSection.subtopics.push(subtopic);
      }
      continue;
    } else if (inTable && !trimmed.startsWith('|')) {
      inTable = false;
    }

    // Resource type headers: **Papers**:, **Libros**:, **Cursos**:, **Repos referencia**:
    const resourceHeaderMatch = trimmed.match(/^\*\*(Papers?|Libros?|Cursos?|Repos?[^*]*|Recursos?[^*]*|Lecturas?[^*]*)\*\*\s*:/i);
    if (resourceHeaderMatch && currentSection) {
      currentResourceType = parseResourceType(resourceHeaderMatch[1]);
      continue;
    }

    // Resource list items: - [label](url) or - *label* — ...
    if (trimmed.startsWith('- ') && currentResourceType && currentSection) {
      const itemText = trimmed.replace(/^-\s+/, '');
      // Skip bold headers that look like list items
      if (itemText.startsWith('**')) continue;

      const { label, url } = extractLinkLabel(itemText);
      if (label && label.length > 1) {
        currentSection.resources.push({
          type: currentResourceType,
          label,
          url,
        });
      }
      continue;
    }

    // Reset resource type on blank line or new heading
    if (!trimmed && currentResourceType) {
      // Don't reset — resources can have blank lines between groups
    }
    if (trimmed.startsWith('#') && !trimmed.startsWith('##')) {
      currentResourceType = null;
    }
    if (trimmed.startsWith('---')) {
      currentResourceType = null;
    }
  }

  // Flush last roadmap day
  flushRoadmapDay();

  return { topics, roadmapDays };
}
