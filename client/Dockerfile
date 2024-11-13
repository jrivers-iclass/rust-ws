# Build stage
FROM node:20-slim as builder

WORKDIR /app

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm install --legacy-peer-deps
RUN npm install @angular/material@^18.2.0 @angular/cdk@^18.2.0 @angular/animations@^18.2.0 --legacy-peer-deps

# Copy the rest of the application
COPY . .

# Build the application
RUN NODE_OPTIONS="--max_old_space_size=4096" npm run build -- --configuration production

# Production stage
FROM nginx:alpine

# Copy the built assets to nginx
COPY --from=builder /app/dist/client/browser /usr/share/nginx/html

# Copy nginx configuration
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"] 