FROM node:20-slim AS builder
WORKDIR /app
COPY package.json ./
RUN npm install
COPY . .
ARG VITE_API_URL
# Per-deployment cut of the panel (unset = full). See src/config/disabled-sections.ts.
ARG VITE_ADMIN_PROFILE
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
