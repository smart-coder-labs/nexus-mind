import type { Meta, StoryObj } from '@storybook/react';
import { SearchInput } from './SearchInput';
import { useState } from 'react';

const meta = {
    title: 'Forms/SearchInput',
    component: SearchInput,
    tags: ['autodocs'],
    argTypes: {
        size: {
            control: 'select',
            options: ['sm', 'md', 'lg'],
        },
        variant: {
            control: 'select',
            options: ['default', 'error'],
        },
        debounceTime: {
            control: 'number',
        },
    },
} satisfies Meta<typeof SearchInput>;

export default meta;
type Story = StoryObj<typeof SearchInput>;

/* ========================================
   STORIES - BASIC
   ======================================== */

export const Default: Story = {
    args: {
        value: '',
        placeholder: 'Search...',
        onChange: () => {},
    },
};

export const WithValue: Story = {
    args: {
        value: 'React hooks',
        placeholder: 'Search topics...',
        onChange: () => {},
    },
};

export const Loading: Story = {
    args: {
        value: 'Searching...',
        isLoading: true,
        placeholder: 'Search...',
        onChange: () => {},
    },
};

export const WithLabel: Story = {
    args: {
        value: '',
        label: 'Search Topics',
        placeholder: 'Search by topic or keyword...',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - SIZES
   ======================================== */

export const SizeSm: Story = {
    args: {
        value: '',
        size: 'sm',
        placeholder: 'Small search...',
        onChange: () => {},
    },
};

export const SizeMd: Story = {
    args: {
        value: '',
        size: 'md',
        placeholder: 'Medium search (default)...',
        onChange: () => {},
    },
};

export const SizeLg: Story = {
    args: {
        value: '',
        size: 'lg',
        placeholder: 'Large search...',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - VARIANTS
   ======================================== */

export const VariantError: Story = {
    args: {
        value: '',
        variant: 'error',
        placeholder: 'Error variant...',
        onChange: () => {},
    },
};

export const VariantErrorWithValue: Story = {
    args: {
        value: 'Invalid search',
        variant: 'error',
        placeholder: 'Error with value...',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - STATES
   ======================================== */

export const Disabled: Story = {
    args: {
        value: 'Search is disabled',
        disabled: true,
        placeholder: 'Disabled search...',
        onChange: () => {},
    },
};

export const DisabledWithLabel: Story = {
    args: {
        value: '',
        disabled: true,
        label: 'Disabled Search',
        placeholder: 'Disabled search...',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - DARK MODE
   ======================================== */

export const DarkMode: Story = {
    parameters: { themes: { themeOverride: 'dark' } },
    args: {
        value: '',
        placeholder: 'Search in dark mode...',
        onChange: () => {},
    },
};

export const DarkModeWithValue: Story = {
    parameters: { themes: { themeOverride: 'dark' } },
    args: {
        value: 'Dark mode search',
        placeholder: 'Search in dark mode...',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - MOBILE
   ======================================== */

export const MobileView: Story = {
    parameters: { viewport: { defaultViewport: 'mobile1' } },
    args: {
        value: '',
        placeholder: 'Mobile search...',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - PLAYGROUND
   ======================================== */

export const Playground: Story = {
    args: {
        value: '',
        placeholder: 'Search playground...',
        size: 'md',
        variant: 'default',
        onChange: () => {},
    },
};

/* ========================================
   STORIES - COMPOUND COMPONENTS (RENDER)
   ======================================== */

export const WithDropdown: Story = {
    parameters: { layout: 'centered' },
    render: () => {
        const [query, setQuery] = useState('React');
        const [showResults, setShowResults] = useState(true);

        const subtopics = [
            { id: '1', label: 'React Hooks', topic_title: 'React', section_title: 'Fundamentals', priority: 'P0' as const, iconType: 'paper' as const },
            { id: '2', label: 'useState', topic_title: 'React', section_title: 'Hooks', priority: 'P1' as const, iconType: 'paper' as const },
            { id: '3', label: 'useEffect', topic_title: 'React', section_title: 'Hooks', priority: 'P1' as const, iconType: 'paper' as const },
            { id: '4', label: 'Custom Hooks', topic_title: 'React', section_title: 'Advanced', priority: 'P2' as const, iconType: 'book' as const },
            { id: '5', label: 'useReducer', topic_title: 'React', section_title: 'Hooks', priority: 'P2' as const, iconType: 'paper' as const },
        ];

        const resources = [
            { id: 'r1', label: 'React Docs', type: 'website' as const, topic_title: 'React', section_title: 'Official' },
            { id: 'r2', label: 'Hooks API Reference', type: 'paper' as const, topic_title: 'React', section_title: 'Docs' },
            { id: 'r3', label: 'React Patterns Course', type: 'course' as const, topic_title: 'React', section_title: 'Learning' },
            { id: 'r4', label: 'Thinking in React', type: 'book' as const, topic_title: 'React', section_title: 'Books' },
        ];

        const filteredSubtopics = query ? subtopics.filter(s => s.label.toLowerCase().includes(query.toLowerCase())) : subtopics;
        const filteredResources = query ? resources.filter(r => r.label.toLowerCase().includes(query.toLowerCase())) : resources;
        const hasResults = filteredSubtopics.length > 0 || filteredResources.length > 0;

        const getBadgeVariant = (priority: string) => {
            if (priority === 'P0') return 'error';
            if (priority === 'P1') return 'warning';
            return 'default';
        };

        return (
            <div className="w-full max-w-md">
                <SearchInput
                    value={query}
                    onChange={(v) => {
                        setQuery(v);
                        setShowResults(v.length > 0);
                    }}
                    onFocus={() => query && setShowResults(true)}
                    onBlur={() => setTimeout(() => setShowResults(false), 200)}
                    isLoading={false}
                    placeholder="Search topics and resources..."
                >
                    <SearchInput.Dropdown show={showResults} hasResults={hasResults} query={query}>
                        {filteredSubtopics.length > 0 && (
                            <SearchInput.Section title="Subtopics">
                                {filteredSubtopics.slice(0, 5).map((s) => (
                                    <SearchInput.Item key={s.id} onClick={() => console.log('Selected:', s.label)}>
                                        <SearchInput.ItemIcon type={s.iconType} />
                                        <SearchInput.ItemContent
                                            label={s.label}
                                            subtitle={`${s.topic_title} · ${s.section_title}`}
                                        />
                                        <SearchInput.TrailingBadge variant={getBadgeVariant(s.priority)}>
                                            {s.priority}
                                        </SearchInput.TrailingBadge>
                                    </SearchInput.Item>
                                ))}
                            </SearchInput.Section>
                        )}

                        {filteredResources.length > 0 && (
                            <SearchInput.Section title="Resources">
                                {filteredResources.slice(0, 4).map((r) => (
                                    <SearchInput.Item key={r.id}>
                                        <SearchInput.ItemIcon type={r.type} />
                                        <SearchInput.ItemContent
                                            label={r.label}
                                            subtitle={`${r.topic_title} · ${r.section_title}`}
                                        />
                                    </SearchInput.Item>
                                ))}
                            </SearchInput.Section>
                        )}
                    </SearchInput.Dropdown>
                </SearchInput>
            </div>
        );
    },
};

export const NoResults: Story = {
    parameters: { layout: 'centered' },
    render: () => {
        const [query, setQuery] = useState('xyznonexistent');

        return (
            <div className="w-full max-w-md">
                <SearchInput
                    value={query}
                    onChange={setQuery}
                    placeholder="Search..."
                >
                    <SearchInput.Dropdown show={true} hasResults={false} query={query}>
                        <div />
                    </SearchInput.Dropdown>
                </SearchInput>
            </div>
        );
    },
};

export const DarkModeWithDropdown: Story = {
    parameters: { 
        layout: 'centered',
        themes: { themeOverride: 'dark' } 
    },
    render: () => {
        const [query, setQuery] = useState('React');

        const subtopics = [
            { id: '1', label: 'React Hooks', topic_title: 'React', section_title: 'Fundamentals', priority: 'P0' as const, iconType: 'paper' as const },
            { id: '2', label: 'useState', topic_title: 'React', section_title: 'Hooks', priority: 'P1' as const, iconType: 'paper' as const },
        ];

        return (
            <div className="w-full max-w-md">
                <SearchInput value={query} onChange={setQuery} placeholder="Search...">
                    <SearchInput.Dropdown show={query.length > 0} hasResults={subtopics.length > 0} query={query}>
                        <SearchInput.Section title="Subtopics">
                            {subtopics.map((s) => (
                                <SearchInput.Item key={s.id} onClick={() => console.log('Selected:', s.label)}>
                                    <SearchInput.ItemIcon type={s.iconType} />
                                    <SearchInput.ItemContent
                                        label={s.label}
                                        subtitle={`${s.topic_title} · ${s.section_title}`}
                                    />
                                    <SearchInput.TrailingBadge variant="error">{s.priority}</SearchInput.TrailingBadge>
                                </SearchInput.Item>
                            ))}
                        </SearchInput.Section>
                    </SearchInput.Dropdown>
                </SearchInput>
            </div>
        );
    },
};

export const WithDebounce: Story = {
    parameters: { layout: 'centered' },
    render: () => {
        const [query, setQuery] = useState('');
        const [debouncedValue, setDebouncedValue] = useState('');

        return (
            <div className="w-full max-w-md space-y-4">
                <SearchInput
                    value={query}
                    onChange={setQuery}
                    debounceTime={500}
                    onSearch={(value) => setDebouncedValue(value)}
                    placeholder="Type to debounce..."
                />
                <p className="text-sm text-text-secondary">
                    Debounced value: <span className="font-mono">{debouncedValue}</span>
                </p>
            </div>
        );
    },
};

export const InteractiveSearch: Story = {
    parameters: { layout: 'centered' },
    render: () => {
        const [query, setQuery] = useState('');
        const [results, setResults] = useState<string[]>([]);
        const [isLoading, setIsLoading] = useState(false);

        const transactions = [
            'Amazon - $89.99', 'Uber Ride - $24.50', 'Netflix - $15.99',
            'Salary Deposit - $4,500', 'Electric Bill - $134.50',
            'Starbucks - $5.75', 'Apple Store - $999.00',
        ];

        const handleSearch = (value: string) => {
            setIsLoading(true);
            setTimeout(() => {
                setResults(transactions.filter(t => t.toLowerCase().includes(value.toLowerCase())));
                setIsLoading(false);
            }, 500);
        };

        return (
            <div className="w-full max-w-md space-y-4">
                <SearchInput
                    value={query}
                    onChange={(v) => {
                        setQuery(v);
                        if (v.length > 0) handleSearch(v);
                        else setResults([]);
                    }}
                    isLoading={isLoading}
                    placeholder="Search transactions..."
                    onClear={() => setResults([])}
                />
                {results.length > 0 && (
                    <div className="space-y-1">
                        {results.map((r, i) => (
                            <div key={i} className="p-2 bg-surface-secondary rounded-lg text-sm flex justify-between">
                                <span>{r.split(' - ')[0]}</span>
                                <span className="font-medium">{r.split(' - ')[1]}</span>
                            </div>
                        ))}
                    </div>
                )}
                {query && results.length === 0 && !isLoading && (
                    <p className="text-sm text-text-secondary text-center">No results found</p>
                )}
            </div>
        );
    },
};

export const MobileWithResults: Story = {
    parameters: { 
        layout: 'centered',
        viewport: { defaultViewport: 'mobile1' } 
    },
    render: () => {
        const [query, setQuery] = useState('React');

        return (
            <div className="p-4 w-full">
                <SearchInput value={query} onChange={setQuery} placeholder="Search...">
                    <SearchInput.Dropdown show={query.length > 0} hasResults={true} query={query}>
                        <SearchInput.Section title="Results">
                            <SearchInput.Item onClick={() => {}}>
                                <SearchInput.ItemIcon type="paper" />
                                <SearchInput.ItemContent label="React Hooks" subtitle="React · Fundamentals" />
                            </SearchInput.Item>
                            <SearchInput.Item onClick={() => {}}>
                                <SearchInput.ItemIcon type="book" />
                                <SearchInput.ItemContent label="useState" subtitle="React · Hooks" />
                            </SearchInput.Item>
                        </SearchInput.Section>
                    </SearchInput.Dropdown>
                </SearchInput>
            </div>
        );
    },
};